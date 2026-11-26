//! 类别管理的紧凑工具栏、图文列表及图标网格。
use crate::{native_dialogs::*, skin_assets::RgbaSurface, AppState, ActiveModal, to_wide};
use std::{cell::Cell, mem, ptr};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_SELECTED};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetCapture, ReleaseCapture, GetCapture, VK_ESCAPE};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

pub unsafe fn populate(hwnd: HWND, font: HFONT, app: &AppState, selected: usize) {
    for (index, (label, id)) in [("新建", ID_CAT_BTN_NEW), ("删除", ID_CAT_BTN_DEL),
        ("改名", ID_CAT_BTN_RENAME), ("加密", ID_CAT_BTN_ENC),
        ("上移", ID_CAT_BTN_UP), ("下移", ID_CAT_BTN_DOWN)].into_iter().enumerate() {
        create_ctrl(hwnd, font, "BUTTON", label, 0, WS_TABSTOP,
            18 + index as i32 * 64, 16, 64, 23, id);
    }
    let list = create_ctrl(hwnd, font, "LISTBOX", "", WS_EX_CLIENTEDGE,
        WS_TABSTOP | WS_VSCROLL | (LBS_NOTIFY | LBS_NOINTEGRALHEIGHT | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS) as u32,
        18, 50, 382, 334, ID_CAT_LIST);
    SendMessageW(list, LB_SETITEMHEIGHT, 0, 24);
    refresh_category_listbox(list, &app.categories, selected);
    let edit = create_ctrl(hwnd, font, "EDIT", "", WS_EX_CLIENTEDGE, WS_TABSTOP | ES_AUTOHSCROLL as u32,
        22, 354, 274, 22, ID_CAT_RENAME_EDIT);
    let save = create_ctrl(hwnd, font, "BUTTON", "保存名称", 0, WS_TABSTOP, 300, 354, 96, 22, ID_CAT_RENAME_OK);
    ShowWindow(edit, SW_HIDE); ShowWindow(save, SW_HIDE);
    create_ctrl(hwnd, font, "STATIC", "修改图标：", 0, 0, 18, 405, 64, 18, -1);
    create_ctrl(hwnd, font, "BUTTON", "", 0, WS_TABSTOP | BS_OWNERDRAW as u32, 80, 401, 26, 24, ID_CAT_BTN_ICON);
    create_ctrl(hwnd, font, "BUTTON", "增加图标", 0, WS_TABSTOP, 110, 402, 64, 22, ID_CAT_ADD_ICON);
    create_ctrl(hwnd, font, "BUTTON", "管理图标", 0, WS_TABSTOP, 178, 402, 64, 22, ID_CAT_MANAGE_ICONS);
    create_ctrl(hwnd, font, "BUTTON", "关闭", 0, WS_TABSTOP | BS_DEFPUSHBUTTON as u32, 328, 402, 72, 22, IDCANCEL);
    windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(list);
}

unsafe fn fill(dc: HDC, rect: &RECT, color: u32) {
    let brush = CreateSolidBrush(color); FillRect(dc, rect, brush); DeleteObject(brush);
}

pub unsafe fn draw(d: &DRAWITEMSTRUCT, app: &AppState, font: HFONT) -> bool {
    let id = d.CtlID as i32;
    if id != ID_CAT_LIST && !(ID_CAT_BTN_NEW..=ID_CAT_BTN_DOWN).contains(&id) && id != ID_CAT_BTN_ICON { return false; }
    let old = SelectObject(d.hDC, font);
    let mode = SetBkMode(d.hDC, TRANSPARENT as i32);
    let color = SetTextColor(d.hDC, 0);
    let mut area = d.rcItem;
    if id == ID_CAT_LIST {
        fill(d.hDC, &area, 0x00ff_ffff);
        if let Some(cat) = app.categories.get(d.itemID as usize) {
            blit_surface_on_bg(d.hDC, app.builtin_icons.category_icon(&cat.icon_name), area.left + 3, area.top + 4, 16, 16, [255; 3]);
            area.left += 23;
            let bold = if d.itemState & ODS_SELECTED != 0 {
                CreateFontW(-12, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 0, 0, to_wide("宋体").as_ptr())
            } else { 0 };
            if bold != 0 { SelectObject(d.hDC, bold); }
            DrawTextW(d.hDC, to_wide(&cat.name).as_ptr(), -1, &mut area, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS);
            if bold != 0 { SelectObject(d.hDC, font); DeleteObject(bold); }
        }
    } else if id == ID_CAT_BTN_ICON {
        fill(d.hDC, &area, crate::advanced_style::PAGE);
        if let ActiveModal::CategoryManager { selected_idx, .. } = &app.modal {
            if let Some(cat) = app.categories.get(*selected_idx) {
                let rgb = crate::advanced_style::PAGE.to_le_bytes();
                blit_surface_on_bg(d.hDC, app.builtin_icons.category_icon(&cat.icon_name), 5, 4, 16, 16, [rgb[0], rgb[1], rgb[2]]);
            }
        }
    } else {
        let selected = d.itemState & ODS_SELECTED != 0;
        let background = if selected { 0x00f2_f2df } else { 0x00ff_ffff };
        fill(d.hDC, &area, background);
        let brush = CreateSolidBrush(0x00dc_d3c9); FrameRect(d.hDC, &area, brush); DeleteObject(brush);
        let (label, icon) = match id {
            ID_CAT_BTN_NEW => ("新建", &app.builtin_icons.ctx_add_file),
            ID_CAT_BTN_DEL => ("删除", &app.builtin_icons.im_delete),
            ID_CAT_BTN_RENAME => ("改名", &app.builtin_icons.im_edit),
            ID_CAT_BTN_ENC => ("加密", &app.builtin_icons.ctx_lock_col),
            ID_CAT_BTN_UP => ("上移", &app.builtin_icons.im_move_top),
            _ => ("下移", &app.builtin_icons.im_move_bottom),
        };
        let rgb = background.to_le_bytes();
        if id == ID_CAT_BTN_RENAME {
            let mut letter = RECT { left: area.left + 5, top: area.top, right: area.left + 22, bottom: area.bottom };
            let glyph = CreateFontW(-14, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 0, 0, to_wide("Arial").as_ptr());
            let previous = SelectObject(d.hDC, glyph);
            SetTextColor(d.hDC, 0x0080_4828);
            DrawTextW(d.hDC, to_wide("A").as_ptr(), -1, &mut letter, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
            SelectObject(d.hDC, previous); DeleteObject(glyph);
            SetTextColor(d.hDC, 0);
        } else {
            blit_surface_on_bg(d.hDC, icon, area.left + 5, area.top + 3, 16, 16, [rgb[0], rgb[1], rgb[2]]);
        }
        area.left += 25;
        DrawTextW(d.hDC, to_wide(label).as_ptr(), -1, &mut area, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
    }
    SetTextColor(d.hDC, color); SetBkMode(d.hDC, mode); SelectObject(d.hDC, old);
    true
}

struct Grid { icons: Vec<(String, RgbaSurface)>, first: Cell<usize>, hover: Cell<Option<usize>>, result: Cell<Option<usize>>, rows: usize }

fn icon_order(a: &str, b: &str) -> std::cmp::Ordering {
    let a = std::path::Path::new(a).file_stem().unwrap_or_default().to_string_lossy();
    let b = std::path::Path::new(b).file_stem().unwrap_or_default().to_string_lossy();
    if a.is_ascii() != b.is_ascii() { return b.is_ascii().cmp(&a.is_ascii()); }
    if a.is_ascii() { return a.to_lowercase().cmp(&b.to_lowercase()); }
    unsafe {
        let left = to_wide(&a); let right = to_wide(&b);
        let order = windows_sys::Win32::Globalization::CompareStringEx(to_wide("zh-CN").as_ptr(),
            windows_sys::Win32::Globalization::NORM_IGNORECASE, left.as_ptr(), -1,
            right.as_ptr(), -1, ptr::null(), ptr::null(), 0);
        order.cmp(&2)
    }
}

unsafe fn apply_selection(owner: HWND, index: usize, key: String, icon: Option<RgbaSurface>) {
    crate::APP_STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                if let Some(cat) = app.categories.get_mut(index) { cat.icon_name = key.clone(); }
                if let Some(icon) = icon { app.builtin_icons.app_icons.insert(key, icon); }
                app.persist_data();
                refresh_category_listbox(GetDlgItem(owner, ID_CAT_LIST), &app.categories, index);
                InvalidateRect(app.main_hwnd, ptr::null(), 0);
            }
        }
    });
    InvalidateRect(GetDlgItem(owner, ID_CAT_BTN_ICON), ptr::null(), 1);
}

pub unsafe fn pick(owner: HWND, index: usize) {
    let directory = crate::APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        Some(app.base_dir.join("icons/small"))
    });
    let Some(directory) = directory else { return; };
    let mut icons = std::fs::read_dir(directory).into_iter().flatten().flatten().filter_map(|entry| {
        let icon = RgbaSurface::load_file(&entry.path())?.resize_to(16, 16);
        Some((format!("icons/small/{}", entry.file_name().to_string_lossy()), icon))
    }).collect::<Vec<_>>();
    icons.sort_by(|a, b| icon_order(&a.0, &b.0));
    if let Some(key) = choose_icon(owner, icons.clone()) {
        let image = icons.into_iter().find(|(name, _)| *name == key).map(|(_, image)| image);
        apply_selection(owner, index, key, image);
    }
}

pub unsafe fn add(owner: HWND, index: usize) {
    let Some(source) = crate::pick_file_dialog(owner, "增加类别图标", &[("图标与图片", "*.ico;*.png;*.bmp;*.gif")]) else { return; };
    let Some(image) = RgbaSurface::load_file(&source) else { return; };

    apply_selection(owner, index, source.to_string_lossy().into_owned(), Some(image.resize_to(16, 16)));
}

pub unsafe fn manage(owner: HWND) {
    let directory = crate::APP_STATE.with(|cell| cell.try_borrow().ok()
        .and_then(|guard| guard.as_ref().map(|app| app.base_dir.join("icons/small"))));
    if let Some(directory) = directory {
        crate::launch_target_url_or_path("explorer.exe", &format!("\"{}\"", directory.display()), "", owner);
    }
}

pub unsafe fn choose_icon(owner: HWND, icons: Vec<(String, RgbaSurface)>) -> Option<String> {
    if icons.is_empty() { return None; }
    let rows = icons.len().div_ceil(4).min(11);
    let grid = Grid { icons, first: Cell::new(0), hover: Cell::new(None), result: Cell::new(None), rows };
    let width = 282;
    let height = (rows as i32 * 25 + 12).max(296);
    let mut anchor: RECT = mem::zeroed(); GetWindowRect(GetDlgItem(owner, ID_CAT_BTN_ICON), &mut anchor);
    let mut monitor: MONITORINFO = mem::zeroed(); monitor.cbSize = mem::size_of_val(&monitor) as u32;
    GetMonitorInfoW(MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST), &mut monitor);
    let x = anchor.left.clamp(monitor.rcWork.left, (monitor.rcWork.right - width).max(monitor.rcWork.left));
    let y = (anchor.top - height).clamp(monitor.rcWork.top, (monitor.rcWork.bottom - height).max(monitor.rcWork.top));
    let class = to_wide("KStartCategoryIconGrid");
    let instance = GetModuleHandleW(ptr::null());
    RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(grid_proc), hInstance: instance, lpszClassName: class.as_ptr(), ..mem::zeroed() });
    let popup = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE, class.as_ptr(), ptr::null(),
        WS_POPUP | WS_BORDER, x, y, width, height, owner, 0, instance, &grid as *const Grid as _);
    if popup == 0 { return None; }
    ShowWindow(popup, SW_SHOWNOACTIVATE); SetCapture(popup);
    let mut message: MSG = mem::zeroed();
    while IsWindow(popup) != 0 && GetCapture() == popup {
        let status = GetMessageW(&mut message, 0, 0, 0);
        if status <= 0 { if status == 0 { PostQuitMessage(message.wParam as i32); } break; }
        if message.message == WM_KEYDOWN && message.wParam as u16 == VK_ESCAPE { break; }
        TranslateMessage(&message); DispatchMessageW(&message);
    }
    if GetCapture() == popup { ReleaseCapture(); }
    if IsWindow(popup) != 0 { DestroyWindow(popup); }
    grid.result.get().map(|index| grid.icons[index].0.clone())
}

unsafe extern "system" fn grid_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*(lp as *const CREATESTRUCTW)).lpCreateParams as isize);
    }
    let Some(grid) = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Grid).as_ref() else { return DefWindowProcW(hwnd, msg, wp, lp); };
    let x = lp as i16 as i32;
    let y = (lp >> 16) as i16 as i32;
    match msg {
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_MOUSEMOVE => {
            let index = grid.first.get() * 4 + ((y - 5).max(0) / 25) as usize * 4 + ((x - 5).max(0) / 68) as usize;
            let next = (x >= 5 && x < 277 && y >= 5 && index < grid.icons.len()).then_some(index);
            if grid.hover.replace(next) != next { InvalidateRect(hwnd, ptr::null(), 0); }
            0
        }
        WM_LBUTTONDOWN => {
            if x < 0 || x >= 282 || y < 0 || y >= grid.rows as i32 * 25 + 12 { DestroyWindow(hwnd); }
            0
        }
        WM_LBUTTONUP => {
            if let Some(index) = grid.hover.get() { grid.result.set(Some(index)); }
            DestroyWindow(hwnd); 0
        }
        WM_MOUSEWHEEL => {
            let max = grid.icons.len().div_ceil(4).saturating_sub(grid.rows);
            grid.first.set(if (wp >> 16) as i16 > 0 { grid.first.get().saturating_sub(2) } else { (grid.first.get() + 2).min(max) });
            grid.hover.set(None); InvalidateRect(hwnd, ptr::null(), 0); 0
        }
        WM_PAINT | WM_PRINTCLIENT => {
            let mut ps: PAINTSTRUCT = mem::zeroed();
            let dc = if msg == WM_PRINTCLIENT { wp as HDC } else { BeginPaint(hwnd, &mut ps) };
            let mut rect: RECT = mem::zeroed(); GetClientRect(hwnd, &mut rect);
            let mut canvas = crate::skin_assets::Canvas::new(rect.right, rect.bottom);
            canvas.fill_rect(0, 0, rect.right, rect.bottom, crate::skin_parser::RgbColor::new(255, 255, 255));
            let mut texts = Vec::new();
            for (local, (name, icon)) in grid.icons.iter().skip(grid.first.get() * 4).take(grid.rows * 4).enumerate() {
                let index = grid.first.get() * 4 + local;
                let x = 5 + (local % 4) as i32 * 68; let y = 5 + (local / 4) as i32 * 25;
                if grid.hover.get() == Some(index) {
                    canvas.fill_rect(x, y, 68, 24, crate::skin_parser::RgbColor::new(213, 229, 245));
                }
                canvas.blit(icon, x + 1, y + 4);
                let name = std::path::Path::new(name).file_stem().unwrap_or_default().to_string_lossy();
                texts.push(crate::renderer::TextDrawCmd { text: name.into_owned(), x: x + 21, y,
                    w: 47, h: 24, color: crate::skin_parser::RgbColor::new(0, 0, 0), bold: false,
                    align: crate::renderer::TextAlign::Left, mnemonic: false, multiline: false, font_slot: None });
            }
            crate::blit_canvas_with_gdi_texts(&canvas, &texts, None, dc, false, None);
            if msg == WM_PAINT { EndPaint(hwnd, &ps); }
            0
        }
        WM_CLOSE => { DestroyWindow(hwnd); 0 }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn capture(hwnd: HWND, width: i32, height: i32, path: &std::path::Path) {
        let dc = CreateCompatibleDC(0);
        let mut info: BITMAPINFO = mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER { biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width, biHeight: -height, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB, ..mem::zeroed() };
        let mut bits = ptr::null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, 0, 0);
        let previous = SelectObject(dc, bitmap);
        SendMessageW(hwnd, WM_PRINT, dc as usize, (PRF_CLIENT | PRF_CHILDREN | PRF_ERASEBKGND) as isize);
        GdiFlush();
        let pixels = std::slice::from_raw_parts(bits as *const u32, (width * height) as usize)
            .iter().map(|pixel| pixel | 0xff00_0000).collect();
        SelectObject(dc, previous); DeleteObject(bitmap); DeleteDC(dc);
        crate::save_bgra_to_png(width as u32, height as u32, pixels, path).unwrap();
    }

    #[test]
    fn export_native_category_previews_when_requested() {
        let Some(directory) = std::env::var_os("KSTART_CATEGORY_PREVIEW_DIR") else { return; };
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/icon-verify");
        let mut app = AppState::new(base, crate::renderer::ActivePopupMenu::None).unwrap();
        app.categories.truncate(2);
        app.categories[0].name = "我的电脑".into(); app.categories[0].icon_name = "cat_laptop".into();
        app.categories[1].name = "编程软件".into(); app.categories[1].icon_name = "cat_cube".into();
        app.modal = ActiveModal::CategoryManager { selected_idx: 1, renaming: false, rename_buf: String::new() };
        let mut icons = app.builtin_icons.app_icons.iter().filter(|(name, _)| name.starts_with("icons/small/"))
            .map(|(name, icon)| (name.clone(), icon.clone())).collect::<Vec<_>>();
        icons.sort_by(|a, b| icon_order(&a.0, &b.0));
        assert!(icons.len() >= 30);
        crate::APP_STATE.with(|cell| *cell.try_borrow_mut().unwrap() = Some(app));
        unsafe {
            let instance = GetModuleHandleW(ptr::null());
            let class = to_wide(crate::NATIVE_MODAL_CLASS_NAME);
            RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(crate::native_modal_wnd_proc), hInstance: instance,
                lpszClassName: class.as_ptr(), hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH, ..mem::zeroed() });
            let owner = CreateWindowExW(0, class.as_ptr(), ptr::null(), WS_POPUP,
                0, 0, 410, 438, 0, 0, instance, ptr::null());
            assert_ne!(owner, 0);
            capture(owner, 410, 438, &directory.join("categories.png"));
            let grid = Grid { rows: icons.len().div_ceil(4).min(11), icons,
                first: Cell::new(0), hover: Cell::new(None), result: Cell::new(None) };
            let grid_class = to_wide("KStartCategoryIconGridPreview");
            RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(grid_proc), hInstance: instance,
                lpszClassName: grid_class.as_ptr(), ..mem::zeroed() });
            let height = (grid.rows as i32 * 25 + 12).max(296);
            let popup = CreateWindowExW(0, grid_class.as_ptr(), ptr::null(), WS_POPUP,
                0, 0, 282, height, owner, 0, instance, &grid as *const Grid as _);
            assert_ne!(popup, 0);
            capture(popup, 282, height, &directory.join("category-icons.png"));
            ValidateRect(popup, ptr::null());
            SendMessageW(popup, WM_MOUSEMOVE, 0, 10 | (10 << 16));
            ValidateRect(popup, ptr::null());
            SendMessageW(popup, WM_MOUSEMOVE, 0, 11 | (11 << 16));
            let stable_hover = GetUpdateRect(popup, ptr::null_mut(), 0) == 0;
            DestroyWindow(popup); DestroyWindow(owner);
            assert!(stable_hover, "同一图标内移动不应反复重绘整个面板");
            // 用真实主窗口过程验证靠边后的最终高度与隐藏行为。
            let class = to_wide("KStartMainBehaviorPreview");
            RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(crate::main_wnd_proc), hInstance: instance,
                lpszClassName: class.as_ptr(), ..mem::zeroed() });
            let main = CreateWindowExW(WS_EX_TOOLWINDOW, class.as_ptr(), ptr::null(), WS_POPUP | WS_THICKFRAME,
                0, 100, 350, 500, 0, 0, instance, ptr::null());
            assert_ne!(main, 0);
            crate::APP_STATE.with(|cell| {
                let mut guard = cell.try_borrow_mut().unwrap();
                let app = guard.as_mut().unwrap();
                app.main_hwnd = main; app.modal = ActiveModal::None;
                app.user_cfg.advanced.set(crate::advanced::Key::StretchDocked, "1".into());
                app.user_cfg.advanced.set(crate::advanced::Key::TrayVisibility, "0".into());
            });
            crate::advanced_window::stretch_docked(main);
            let mut stretched: RECT = mem::zeroed(); GetWindowRect(main, &mut stretched);
            let mut monitor: MONITORINFO = mem::zeroed(); monitor.cbSize = mem::size_of_val(&monitor) as u32;
            GetMonitorInfoW(MonitorFromRect(&stretched, MONITOR_DEFAULTTONEAREST), &mut monitor);
            crate::advanced_window::minimize_main(main);
            let hidden = IsWindowVisible(main) == 0 && IsIconic(main) == 0;
            let logo = LoadIconW(instance, 1usize as *const u16);
            crate::APP_STATE.with(|cell| cell.try_borrow_mut().unwrap().as_mut().unwrap().user_cfg.enable_list = false);
            crate::popup::register(instance);
            crate::advanced_window::tray_click(main, WM_RBUTTONUP);
            let menu = GetCapture();
            let opened = menu != 0 && menu != main && IsWindowVisible(menu) != 0;
            SendMessageW(main, WM_ACTIVATE, 0, menu);
            SendMessageW(main, WM_MOVE, 0, 0);
            crate::advanced_window::tick(main);
            let survives_hidden_owner = IsWindowVisible(menu) != 0 && IsWindowVisible(main) == 0;
            crate::popup::dismiss(main);
            crate::advanced_window::remove_tray(main);
            DestroyWindow(main);
            assert_eq!(stretched.top, monitor.rcWork.top);
            assert_eq!(stretched.bottom, monitor.rcWork.bottom);
            assert!(hidden, "最小化到托盘必须隐藏而非创建最小化小窗口");
            assert_ne!(logo, 0, "应用必须嵌入鲸鱼图标资源");
            assert!(opened && survives_hidden_owner, "主窗口隐藏且列表停用时，托盘右键菜单仍须显示");
        }
        let mut app = crate::APP_STATE.with(|cell| cell.try_borrow_mut().unwrap().take()).unwrap();
        let temporary = std::env::temp_dir().join(format!("kstart-batch-drop-{}", std::process::id()));
        std::fs::create_dir_all(temporary.join("user")).unwrap();
        app.base_dir = temporary.clone();
        let count: usize = app.categories.iter().flat_map(|category| &category.groups).map(|group| group.items.len()).sum();
        let files = ["alpha.exe", "beta.exe"].map(|name| crate::advanced_runtime::DroppedFile {
            name: name.into(), target: name.into(), args: String::new(), work_dir: String::new(),
            icon: None, icon_source: name.into(),
        }).into_iter().collect();
        app.add_dropped_files(files);
        let persisted: crate::model::AppDataStore = serde_json::from_slice(&std::fs::read(temporary.join("user/data.json")).unwrap()).unwrap();
        assert_eq!(persisted.shortcuts.len(), count + 2);
        let unique = persisted.shortcuts.iter().map(|item| &item.id).collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), persisted.shortcuts.len());
        let _ = std::fs::remove_file(temporary.join("user/data.json"));
        let _ = std::fs::remove_dir(temporary.join("user"));
        let _ = std::fs::remove_dir(temporary);
    }
}
