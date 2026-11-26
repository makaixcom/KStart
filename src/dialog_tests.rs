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
/// 系统菜单由当前 UI 线程创建，按系统窗口类查找，避免依赖鼠标捕获实现。
pub(crate) unsafe fn menu_window() -> HWND {
    unsafe extern "system" fn find(hwnd: HWND, lp: LPARAM) -> i32 {
        let mut name = [0u16; 32];
        let len = GetClassNameW(hwnd, name.as_mut_ptr(), 32);
        if String::from_utf16_lossy(&name[..len.max(0) as usize]) == "#32768" {
            *(lp as *mut HWND) = hwnd;
            return 0;
        }
        1
    }
    let mut result = 0;
    EnumThreadWindows(windows_sys::Win32::System::Threading::GetCurrentThreadId(),
        Some(find), &mut result as *mut _ as LPARAM);
    result
}

type Step = Box<dyn FnOnce(HWND)>;

#[test]
fn native_controls_and_editing_when_requested() {
    let Some(base) = std::env::var_os("KSTART_NATIVE_UI_BASE") else { return; };
    let base = PathBuf::from(base);
    let shots = base.join("native-shots");
    fs::create_dir_all(&shots).unwrap();
    let app = AppState::new(base, ActivePopupMenu::None).unwrap();
    let edit = app.make_edit_shortcut_modal(0, 0, 0).unwrap();
    let settings = app.user_cfg.advanced.clone();
    APP_STATE.with(|cell| *cell.try_borrow_mut().unwrap() = Some(app));
    unsafe {
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES | ICC_TAB_CLASSES | ICC_LINK_CLASS,
        });
        let class = to_wide(NATIVE_MODAL_CLASS_NAME);
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(native_modal_wnd_proc),
            hInstance: GetModuleHandleW(ptr::null()),
            hCursor: LoadCursorW(0, IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        });
        for (name, modal, input) in [
            ("about", ActiveModal::About, 1703),
            ("shortcut", edit, native_dialogs::ID_EDIT_TITLE),
            ("settings", ActiveModal::Advanced(settings), 0),
            ("categories", ActiveModal::CategoryManager {
                selected_idx: 0, renaming: false, rename_buf: String::new(),
            }, native_dialogs::ID_CAT_RENAME_EDIT),
        ] {
            show_centered_modal(modal);
            let hwnd = APP_STATE.with(|cell| cell.try_borrow().unwrap().as_ref().unwrap().modal_hwnd);
            assert_ne!(hwnd, 0);
            assert_ne!(GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION, 0);
            if input != 0 {
                let edit = GetDlgItem(hwnd, input);
                let mut class = [0u16; 32];
                let length = GetClassNameW(edit, class.as_mut_ptr(), class.len() as i32);
                assert_eq!(String::from_utf16_lossy(&class[..length as usize]), "Edit");
                if name == "about" {
                    assert_ne!(GetWindowLongW(edit, GWL_STYLE) as u32 & ES_READONLY as u32, 0);
                    assert_ne!(GetDlgItem(hwnd, 1701), 0, "应创建原生 SysLink");
                    let mut client: RECT = std::mem::zeroed();
                    let mut bounds: RECT = std::mem::zeroed();
                    GetClientRect(edit, &mut client);
                    GetWindowRect(edit, &mut bounds);
                    MapWindowPoints(edit, 0, &mut client as *mut RECT as *mut POINT, 2);
                    assert_eq!(bounds.right - client.right - (client.left - bounds.left),
                        dialog_scrollbars::WIDTH, "文本框应只为窄滚动条预留空间");
                    let original = native_dialogs::get_ctrl_text(hwnd, input);
                    native_dialogs::set_ctrl_text(hwnd, input, &format!("{original}{}", "\r\n滚动验证".repeat(30)));
                    unsafe extern "system" fn find_bar(bar: HWND, lp: LPARAM) -> i32 {
                        let (source, result) = &mut *(lp as *mut (HWND, HWND));
                        if GetPropW(bar, to_wide(dialog_scrollbars::SOURCE).as_ptr()) == *source
                            && GetWindowLongW(bar, GWL_STYLE) as u32 & SBS_VERT as u32 != 0 {
                            *result = bar;
                        }
                        1
                    }
                    let mut pair = (edit, 0 as HWND);
                    EnumChildWindows(hwnd, Some(find_bar), &mut pair as *mut _ as LPARAM);
                    assert_ne!(pair.1, 0, "文本框应有独立的窄原生滚动条");
                    let mut bar_bounds: RECT = std::mem::zeroed();
                    GetWindowRect(pair.1, &mut bar_bounds);
                    assert_eq!(bar_bounds.right - bar_bounds.left, dialog_scrollbars::WIDTH);
                    let top = SendMessageW(edit, EM_GETFIRSTVISIBLELINE, 0, 0);
                    SendMessageW(hwnd, WM_VSCROLL, SB_LINEDOWN as usize, pair.1);
                    assert!(SendMessageW(edit, EM_GETFIRSTVISIBLELINE, 0, 0) > top);
                    native_dialogs::set_ctrl_text(hwnd, input, &original);
                    let text = native_dialogs::get_ctrl_text(hwnd, input);
                    SendMessageW(edit, WM_CHAR, b'X' as usize, 0);
                    assert_eq!(native_dialogs::get_ctrl_text(hwnd, input), text);
                } else {
                    native_dialogs::set_ctrl_text(hwnd, input, "中文😀输入");
                    SendMessageW(edit, EM_SETSEL, 2, 4);
                    SendMessageW(edit, EM_REPLACESEL, 1, to_wide("替换").as_ptr() as isize);
                    assert_eq!(native_dialogs::get_ctrl_text(hwnd, input), "中文替换输入");
                    assert_ne!(SendMessageW(edit, WM_UNDO, 0, 0), 0);
                    assert_eq!(native_dialogs::get_ctrl_text(hwnd, input), "中文😀输入");
                    SendMessageW(edit, WM_CHAR, 1, 0);
                    let (mut start, mut end) = (0u32, 0u32);
                    SendMessageW(edit, EM_GETSEL, &mut start as *mut _ as usize, &mut end as *mut _ as isize);
                    assert_eq!((start, end), (0, 6), "{name}: Ctrl+A 应全选");
                }
            }
            if name == "settings" {
                for page in 0..advanced::PAGES.len() {
                    advanced_dialog::select_page(hwnd, page);
                    RedrawWindow(hwnd, ptr::null(), 0, RDW_INVALIDATE | RDW_UPDATENOW | RDW_ALLCHILDREN | RDW_ERASE);
                    capture(hwnd, &shots.join(format!("settings-{page}.png")));
                }
            }
            let button = GetDlgItem(hwnd, IDCANCEL);
            assert_ne!(button, 0);
            assert_ne!(GetWindowLongW(button, GWL_STYLE) as u32 & 0xf, BS_OWNERDRAW as u32);
            RedrawWindow(hwnd, ptr::null(), 0, RDW_INVALIDATE | RDW_UPDATENOW | RDW_ALLCHILDREN | RDW_ERASE);
            capture(hwnd, &shots.join(format!("{name}.png")));
            SendMessageW(button, BM_CLICK, 0, 0);
            assert_eq!(IsWindow(hwnd), 0, "取消按钮应关闭窗口");
        }
    }
    APP_STATE.with(|cell| *cell.try_borrow_mut().unwrap() = None);
}
thread_local! {
    static ACTIVE:Cell<bool>=const{Cell::new(false)};
    static STEPS:RefCell<VecDeque<Step>>=RefCell::new(VecDeque::new());
}
pub(crate) fn enable(value: bool) { ACTIVE.with(|active| active.set(value)); }
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
pub(crate) fn queue(step: impl FnOnce(HWND) + 'static) {
    STEPS.with(|steps| steps.try_borrow_mut().unwrap().push_back(Box::new(step)));
}

pub(crate) unsafe fn capture(hwnd: HWND, path: &Path) {
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
        // 构造足够多的列表行，使滚动验收不依赖安装的皮肤数量。
        for index in 0..60 {
            SendMessageW(list, LB_ADDSTRING, 0, to_wide(&format!("滚动验证皮肤 {index}")).as_ptr() as isize);
        }
        let top = SendMessageW(list, LB_GETTOPINDEX, 0, 0);
        SendMessageW(list, WM_MOUSEWHEEL, ((-120i16) as u16 as usize) << 16, 0);
        assert!(
            SendMessageW(list, LB_GETTOPINDEX, 0, 0) > top,
            "皮肤列表应支持滚轮滚动"
        );
        let bar = GetDlgItem(skin, skin_manager::SCROLL);
        let mut bar_class = [0u16; 32];
        let len = GetClassNameW(bar, bar_class.as_mut_ptr(), 32);
        assert_eq!(String::from_utf16_lossy(&bar_class[..len as usize]), "ScrollBar");
        let mut bar_bounds: RECT = std::mem::zeroed();
        GetWindowRect(bar, &mut bar_bounds);
        assert_eq!(bar_bounds.right - bar_bounds.left, dialog_scrollbars::WIDTH);
        let top = SendMessageW(list, LB_GETTOPINDEX, 0, 0);
        SendMessageW(skin, WM_VSCROLL, SB_LINEDOWN as usize, bar);
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

#[test]
fn deleting_last_column_confirms_and_keeps_original_files_when_requested() {
    let Some(base)=std::env::var_os("KSTART_COLUMN_DELETE_UI_BASE")else{return;};
    let base=PathBuf::from(base);let shots=base.join("shots");fs::create_dir_all(&shots).unwrap();
    let original=base.join("original-file.txt");fs::write(&original,"保留原文件").unwrap();
    let mut app=AppState::new(base.clone(),ActivePopupMenu::None).unwrap();
    app.categories[0].groups.truncate(1);app.categories[0].active_group_index=0;
    let mut item=app.categories[0].groups[0].items[0].clone();item.id="sc_delete_column_test".into();item.target_path=original.to_string_lossy().into_owned();item.icon_key=item.target_path.clone();item.custom_icon_16=None;
    app.categories[0].groups[0].items=vec![item];app.selected_shortcuts.insert(0);app.persist_data();
    let category_id=app.categories[0].id.clone();let other_count=app.categories[1].groups.len();
    let data_before=fs::read(base.join("user/data.json")).unwrap();
    let index=build_context_menu_items().iter().position(|item|matches!(item.action,ContextMenuAction::DeleteColumn)).unwrap();
    let (_,request)=handle_context_menu_action(&mut app,index);assert!(matches!(request,DeferredDialogRequest::DeleteColumn(0,0)));assert_eq!(app.categories[0].groups.len(),1);
    APP_STATE.with(|cell|*cell.try_borrow_mut().unwrap()=Some(app));enable(true);
    unsafe{
        let controls=INITCOMMONCONTROLSEX{dwSize:std::mem::size_of::<INITCOMMONCONTROLSEX>()as u32,dwICC:ICC_LISTVIEW_CLASSES};InitCommonControlsEx(&controls);
        let data_path=base.join("user/data.json");
        queue(move|hwnd|{arm(hwnd);confirm_delete_column(hwnd,0,0);assert_eq!(fs::read(&data_path).unwrap(),data_before);APP_STATE.with(|cell|assert_eq!(cell.try_borrow().unwrap().as_ref().unwrap().categories[0].groups.len(),1));arm(hwnd);});
        let shot=shots.join("column-delete-confirm.png");queue(move|hwnd|{
            let confirm=GetLastActivePopup(hwnd);assert_ne!(confirm,hwnd);assert_ne!(IsWindowVisible(confirm),0);
            let mut child=FindWindowExW(confirm,0,to_wide("Static").as_ptr(),ptr::null());let mut found=false;
            while child!=0 {let mut text=[0u16;512];let length=GetWindowTextW(child,text.as_mut_ptr(),text.len()as i32);let text=String::from_utf16_lossy(&text[..length.max(0)as usize]);if text.contains("确定删除栏目")&&text.contains("原文件不受影响"){found=true;}child=FindWindowExW(confirm,child,to_wide("Static").as_ptr(),ptr::null());}
            assert!(found,"确认框应说明删除对象及保留原文件");
            RedrawWindow(confirm,ptr::null(),0,RDW_INVALIDATE|RDW_UPDATENOW|RDW_ALLCHILDREN);capture(confirm,&shot);SendMessageW(confirm,WM_COMMAND,IDNO as usize,0);
        });
        queue(|hwnd|{arm(hwnd);confirm_delete_column(hwnd,0,0);APP_STATE.with(|cell|{let guard=cell.try_borrow().unwrap();let app=guard.as_ref().unwrap();assert!(app.categories[0].groups.is_empty());assert!(app.selected_shortcuts.is_empty());assert_eq!(app.categories[0].active_group_index,0);assert!(renderer::shortcut_grid(&app.skin_cfg,app.user_cfg.window_width,app.user_cfg.window_height,&app.categories[0],&app.user_cfg.advanced).is_none());});arm(hwnd);});
        queue(|hwnd|{let confirm=GetLastActivePopup(hwnd);assert_ne!(confirm,hwnd);SendMessageW(confirm,WM_COMMAND,IDYES as usize,0);});
        queue(|hwnd|{SendMessageW(hwnd,WM_COMMAND,IDCANCEL as usize,0);});
        column_dialog::open(0,0,0,column_dialog::Action::Rename);
    }
    enable(false);
    assert!(original.is_file());
    let saved=AppDataStore::load(&base);assert!(!saved.columns.iter().any(|column|column.category_id==category_id));assert!(!saved.shortcuts.iter().any(|item|item.category_id==category_id));
    let tree=saved.build_runtime_tree(&base,&Default::default(),&Default::default());assert!(tree[0].groups.is_empty());assert_eq!(tree[1].groups.len(),other_count);
    APP_STATE.with(|cell|{
        let mut guard=cell.try_borrow_mut().unwrap();let app=guard.as_mut().unwrap();
        let _draft=app.make_new_shortcut_modal(0,0,"取消的新建".into(),String::new(),String::new(),None,"");assert!(app.categories[0].groups.is_empty());
        app.add_dropped_files(vec![advanced_runtime::DroppedFile{name:"原文件".into(),target:original.to_string_lossy().into_owned(),args:String::new(),work_dir:String::new(),icon:None,icon_source:original.to_string_lossy().into_owned()}]);
        assert_eq!(app.categories[0].groups.len(),1);assert_eq!(app.categories[0].groups[0].items.len(),1);
        let ids=app.categories.iter().flat_map(|cat|&cat.groups).map(|column|&column.id).collect::<HashSet<_>>();assert_eq!(ids.len(),app.categories.iter().map(|cat|cat.groups.len()).sum::<usize>());
    });
    assert_eq!(AppDataStore::load(&base).shortcuts.iter().filter(|item|item.category_id==category_id).count(),1);
    assert!(!base.join("user/icons").exists());assert!(!base.join("user/icon-cache").exists());
    APP_STATE.with(|cell|{cell.try_borrow_mut().unwrap().take();});
}
