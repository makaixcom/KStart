//! 可选的同进程原生窗口回归：设置 KSTART_UI_BASE 和 KSTART_UI_SHOTS 后启用。
use crate::*;
use std::{
    cell::Cell,
    collections::VecDeque,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Graphics::Gdi::*,
    UI::{Controls::*, WindowsAndMessaging::*},
};
const TIMER: usize = 0x4b5354;
#[link(name = "user32")]
extern "system" {
    fn PrintWindow(hwnd: HWND, dc: HDC, flags: u32) -> i32;
}
type Step = Box<dyn FnOnce(HWND)>;
thread_local! {
    static ACTIVE:Cell<bool>=const{Cell::new(false)};
    static STEPS:RefCell<VecDeque<Step>>=RefCell::new(VecDeque::new());
}
pub fn active() -> bool {
    ACTIVE.with(Cell::get)
}
pub unsafe fn arm(hwnd: HWND) {
    if active() {
        SetTimer(hwnd, TIMER, 30, None);
    }
}
pub unsafe fn step(hwnd: HWND, id: usize) -> bool {
    if id != TIMER {
        return false;
    }
    KillTimer(hwnd, TIMER);
    let next = STEPS.with(|steps| steps.try_borrow_mut().unwrap().pop_front());
    if let Some(next) = next {
        next(hwnd);
    } else {
        PostMessageW(hwnd, WM_COMMAND, IDCANCEL as usize, 0);
    }
    true
}
fn queue(step: impl FnOnce(HWND) + 'static) {
    STEPS.with(|steps| steps.try_borrow_mut().unwrap().push_back(Box::new(step)));
}

unsafe fn capture(hwnd: HWND, path: &Path) {
    let mut rect: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rect);
    let (w, h) = (rect.right, rect.bottom);
    let dc = CreateCompatibleDC(0);
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..std::mem::zeroed()
    };
    let mut bits = ptr::null_mut();
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, 0, 0);
    assert_ne!(bitmap, 0);
    let old = SelectObject(dc, bitmap);
    assert_ne!(PrintWindow(hwnd, dc, 1), 0);
    GdiFlush();
    let pixels = std::slice::from_raw_parts(bits as *const u32, (w * h) as usize).to_vec();
    SelectObject(dc, old);
    DeleteObject(bitmap);
    DeleteDC(dc);
    save_bgra_to_png(w as u32, h as u32, pixels, path).unwrap();
}
unsafe fn tab(hwnd: HWND, index: usize) {
    let control = GetDlgItem(hwnd, 1800);
    SendMessageW(control, TCM_SETCURSEL, index, 0);
    let header = NMHDR {
        hwndFrom: control,
        idFrom: 1800,
        code: TCN_SELCHANGE,
    };
    SendMessageW(hwnd, WM_NOTIFY, 1800, &header as *const _ as isize);
}

#[test]
fn native_column_and_skin_workflows_when_requested() {
    let Some(base) = std::env::var_os("KSTART_UI_BASE") else {
        return;
    };
    let base = PathBuf::from(base);
    let shots = PathBuf::from(std::env::var_os("KSTART_UI_SHOTS").expect("缺少截图目录"));
    fs::create_dir_all(&shots).unwrap();
    let mut app = AppState::new(base.clone(), ActivePopupMenu::None).unwrap();
    let template = app.categories[0].groups[0].items[0].clone();
    app.categories[0].groups[0].items = (0..17)
        .map(|index| {
            let mut item = template.clone();
            item.id = format!("sc_{}", index + 100);
            item.name = format!("软件{:02}", 17 - index);
            item.target_kind = targets::TargetKind::File;
            item.target_path = format!(r"D:\soft\App{index}\app.exe");
            item.work_dir = format!(r"D:\soft\App{index}");
            item.icon_key = item.target_path.clone();
            item.column_added_at = 1000 + index;
            item.minimize_on_run = false;
            item
        })
        .collect();
    app.persist_data();
    APP_STATE.with(|cell| *cell.try_borrow_mut().unwrap() = Some(app));
    ACTIVE.with(|value| value.set(true));
    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES | ICC_TAB_CLASSES,
        };
        InitCommonControlsEx(&icc);
        let shot = shots.join("column-rename-native.png");
        queue(move |hwnd| {
            native_dialogs::set_ctrl_text(hwnd, 1820, "办公软件");
            assert_eq!(native_dialogs::get_ctrl_text(hwnd, 1820), "办公软件");
            capture(hwnd, &shot);
            PostMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
        });
        column_dialog::open(0, 0, 0, column_dialog::Action::Rename);
        APP_STATE.with(|cell| {
            assert_eq!(
                cell.try_borrow().unwrap().as_ref().unwrap().categories[0].groups[0].title,
                "办公软件"
            )
        });
        let shot = shots.join("column-sort-native.png");
        queue(move |hwnd| {
            capture(hwnd, &shot);
            SendMessageW(GetDlgItem(hwnd, 1832), BM_CLICK, 0, 0);
            SendMessageW(GetDlgItem(hwnd, 1834), BM_CLICK, 0, 0);
            PostMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
        });
        column_dialog::open(0, 0, 0, column_dialog::Action::Sort);
        APP_STATE.with(|cell| {
            assert_eq!(
                cell.try_borrow().unwrap().as_ref().unwrap().categories[0].groups[0].items[0]
                    .column_added_at,
                1016
            )
        });
        let root = base.clone();
        let directory = shots.clone();
        queue(move |hwnd| {
            capture(hwnd, &directory.join("column-batch-native.png"));
            SendMessageW(hwnd, WM_COMMAND, 1806, 0);
            assert_eq!(
                SendMessageW(GetDlgItem(hwnd, 1801), LVM_GETSELECTEDCOUNT, 0, 0),
                17
            );
            SendMessageW(hwnd, WM_COMMAND, 1805, 0);
            APP_STATE.with(|cell| {
                assert!(
                    cell.try_borrow().unwrap().as_ref().unwrap().categories[0].groups[0]
                        .items
                        .iter()
                        .all(|item| item.minimize_on_run)
                )
            });
            SendMessageW(hwnd, WM_COMMAND, 1806, 0);
            SendMessageW(hwnd, WM_COMMAND, 1803, 0);
            APP_STATE.with(|cell| {
                assert!(
                    cell.try_borrow().unwrap().as_ref().unwrap().categories[0].groups[0]
                        .items
                        .iter()
                        .all(|item| item.target_path.starts_with(r"E:\Apps\"))
                )
            });
            tab(hwnd, 1);
            capture(hwnd, &directory.join("column-export-native.png"));
            tab(hwnd, 2);
            capture(hwnd, &directory.join("column-import-native.png"));
            let import = root.join("import");
            fs::create_dir_all(import.join("nested")).unwrap();
            fs::write(
                import.join("one.url"),
                "[InternetShortcut]\r\nURL=https://example.com/one\r\n",
            )
            .unwrap();
            fs::write(
                import.join("nested/two.url"),
                "[InternetShortcut]\r\nURL=https://example.com/two\r\n",
            )
            .unwrap();
            native_dialogs::set_ctrl_text(hwnd, 1810, &import.to_string_lossy());
            SendMessageW(hwnd, WM_COMMAND, 1813, 0);
            let deadline = Instant::now() + Duration::from_secs(5);
            while native_dialogs::get_ctrl_text(hwnd, 1813) != "开始(&S)"
                && Instant::now() < deadline
            {
                let mut message: MSG = std::mem::zeroed();
                while PeekMessageW(&mut message, 0, 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(
                SendMessageW(GetDlgItem(hwnd, 1801), LVM_GETITEMCOUNT, 0, 0),
                2
            );
            capture(hwnd, &directory.join("column-import-results-native.png"));
            SendMessageW(hwnd, WM_COMMAND, 1806, 0);
            SendMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
            let store = AppDataStore::load(&root);
            let web: Vec<_> = store
                .shortcuts
                .iter()
                .filter(|item| item.target_kind == Some(targets::TargetKind::Web))
                .collect();
            assert_eq!(web.len(), 2);
            assert!(web.iter().all(|item| item.column_added_at > 1016));
            PostMessageW(hwnd, WM_COMMAND, IDCANCEL as usize, 0);
        });
        queue(|hwnd| {
            SendMessageW(GetDlgItem(hwnd, 1835), BM_CLICK, 0, 0);
            PostMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
        });
        queue(|hwnd| {
            native_dialogs::set_ctrl_text(hwnd, 1821, r"D:\soft");
            native_dialogs::set_ctrl_text(hwnd, 1822, r"E:\Apps");
            PostMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
        });
        column_dialog::open(0, 0, 0, column_dialog::Action::Batch);
        APP_STATE.with(|cell| {
            cell.try_borrow_mut().unwrap().as_mut().unwrap().modal = ActiveModal::SkinManager
        });
        let class = to_wide(NATIVE_MODAL_CLASS_NAME);
        let instance = GetModuleHandleW(ptr::null());
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(native_modal_wnd_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        });
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
        let mut bounds = RECT {
            right: 760,
            bottom: 680,
            ..std::mem::zeroed()
        };
        AdjustWindowRectEx(&mut bounds, style, 0, 0);
        let skin = CreateWindowExW(
            0,
            class.as_ptr(),
            to_wide("皮肤管理器").as_ptr(),
            style,
            0,
            0,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            0,
            0,
            instance,
            ptr::null(),
        );
        assert_ne!(skin, 0);
        dialog_layout::install(skin, false);
        skin_manager::arrange(skin, 760, 680, 194);
        ShowWindow(skin, SW_SHOWNOACTIVATE);
        RedrawWindow(
            skin,
            ptr::null(),
            0,
            RDW_INVALIDATE | RDW_UPDATENOW | RDW_ALLCHILDREN | RDW_ERASE,
        );
        capture(skin, &shots.join("skin-local-native.png"));
        let list = GetDlgItem(skin, native_dialogs::ID_SKIN_LIST);
        let top = SendMessageW(list, LB_GETTOPINDEX, 0, 0);
        SendMessageW(list, WM_MOUSEWHEEL, ((-120i16) as u16 as usize) << 16, 0);
        assert!(
            SendMessageW(list, LB_GETTOPINDEX, 0, 0) > top,
            "皮肤列表应支持滚轮滚动"
        );
        let bar = GetDlgItem(skin, skin_manager::SCROLL);
        let mut bar_rect: RECT = std::mem::zeroed();
        GetClientRect(bar, &mut bar_rect);
        let top = SendMessageW(list, LB_GETTOPINDEX, 0, 0);
        SendMessageW(
            bar,
            WM_LBUTTONDOWN,
            1,
            7 | ((bar_rect.bottom - 5) << 16) as isize,
        );
        assert!(
            SendMessageW(list, LB_GETTOPINDEX, 0, 0) > top,
            "共用滚动条下箭头应滚动列表"
        );
        let tab = GetDlgItem(skin, skin_manager::TAB);
        SendMessageW(tab, TCM_SETCURSEL, 1, 0);
        let header = NMHDR {
            hwndFrom: tab,
            idFrom: skin_manager::TAB as usize,
            code: TCN_SELCHANGE,
        };
        SendMessageW(skin, WM_NOTIFY, header.idFrom, &header as *const _ as isize);
        capture(skin, &shots.join("skin-network-native.png"));
        skin_manager::arrange(skin, 608, 544, 384);
        for id in [
            native_dialogs::ID_SKIN_APPLY,
            skin_manager::DELETE,
            IDCANCEL,
        ] {
            let mut rect: RECT = std::mem::zeroed();
            GetWindowRect(GetDlgItem(skin, id), &mut rect);
            MapWindowPoints(0, skin, &mut rect as *mut RECT as *mut POINT, 2);
            assert!(
                rect.left >= 384 && rect.right <= 608 && rect.bottom <= 544,
                "右侧按钮应保持在缩小后的右栏范围内"
            );
        }
        DestroyWindow(skin);
    }
    ACTIVE.with(|value| value.set(false));
    APP_STATE.with(|cell| cell.try_borrow_mut().unwrap().take());
}
