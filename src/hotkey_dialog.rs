//! 热键管理、单条编辑、系统功能选择和文本输出窗口。

use crate::{
    advanced_style,
    config::HotkeyConfig,
    hotkeys::{self, Action, Binding, Key},
    native_dialogs::{blit_surface_on_bg, create_ctrl, get_ctrl_text, set_ctrl_text},
    skin_assets::RgbaSurface,
    targets::{self, PathMode, TargetKind},
    to_wide, APP_STATE,
};
use std::{cell::RefCell, mem, path::PathBuf, ptr, sync::OnceLock};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT, WPARAM},
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::*,
        Input::KeyboardAndMouse::*,
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

const LIST: i32 = 1301;
const ADD: i32 = 1302;
const DELETE: i32 = 1303;
const MODIFY: i32 = 1304;
const FILTER: i32 = 1305;
const STATUS: i32 = 1306;
const KEY: i32 = 1311;
const LINK: i32 = 1312;
const ARGS: i32 = 1313;
const START: i32 = 1314;
const REMARK: i32 = 1315;
const CONVERT: i32 = 1316;
const FILE: i32 = 1320;
const FOLDER: i32 = 1321;
const SYSTEM: i32 = 1322;
const TEXT: i32 = 1323;
const CONTROLS: i32 = 1330;
const TEXT_BODY: i32 = 1331;
const TAB: i32 = 1340;
const SYSTEM_LEFT: i32 = 1341;
const SYSTEM_RIGHT: i32 = 1342;

const FILTERS: &[&str] = &[
    "所有",
    "文件",
    "文件夹",
    "系统功能",
    "浏览器链接",
    "音速启动",
    "文本",
    "无效",
];
const ICONS: [(i32, &[u8], &str); 4] = [
    (
        FILE,
        include_bytes!("../assets/icons/ctx_add_file.png"),
        "添加文件",
    ),
    (
        FOLDER,
        include_bytes!("../assets/icons/ctx_add_dir.png"),
        "添加文件夹",
    ),
    (
        SYSTEM,
        include_bytes!("../assets/icons/ctx_add_sys.png"),
        "系统功能",
    ),
    (
        TEXT,
        include_bytes!("../assets/icons/im_edit.png"),
        "热键输出文本",
    ),
];

struct Manager {
    main: HWND,
    base: PathBuf,
    config: HotkeyConfig,
    filter: usize,
    visible: Vec<usize>,
}

struct Editor {
    base: PathBuf,
    key: String,
    action: Option<Action>,
    remark: String,
    enabled: bool,
    initial_link: String,
    initial_args: String,
    initial_start: String,
    used_keys: Vec<Key>,
}

enum Dialog {
    Manager(Manager),
    Editor(Editor),
    Text(String),
    System {
        tab: usize,
        selected: Option<Choice>,
    },
}

pub fn open(shortcut: Option<&str>) {
    let snapshot = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        if app.user_cfg.startup_lock {
            return None;
        }
        Some(Manager {
            main: app.main_hwnd,
            base: app.base_dir.clone(),
            config: app.hotkey_cfg.clone(),
            filter: 0,
            visible: Vec::new(),
        })
    });
    let Some(mut manager) = snapshot else {
        return;
    };
    unsafe {
        if let Some(id) = shortcut {
            let existing = manager
                .config
                .bindings
                .iter()
                .position(|(_, binding)| binding.references(id));
            edit(manager.main, &mut manager, existing, Some(id));
        } else {
            let owner = manager.main;
            show(owner, "热键管理", (576, 500), Dialog::Manager(manager));
        }
    }
}

unsafe fn show(owner: HWND, title: &str, size: (i32, i32), dialog: Dialog) -> Option<Dialog> {
    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_CENTER as u32;
    let mut template = vec![
        style as u16,
        (style >> 16) as u16,
        0,
        0,
        0,
        0,
        0,
        100,
        100,
        0,
        0,
    ];
    template.extend(to_wide(title));
    let state = RefCell::new((dialog, size));
    let result = DialogBoxIndirectParamW(
        GetModuleHandleW(ptr::null()),
        template.as_ptr().cast(),
        owner,
        Some(dialog_proc),
        &state as *const _ as LPARAM,
    );
    (result == IDOK as isize).then(|| state.into_inner().0)
}

unsafe fn center(hwnd: HWND, (width, height): (i32, i32)) {
    let mut bounds = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    AdjustWindowRectEx(
        &mut bounds,
        GetWindowLongW(hwnd, GWL_STYLE) as u32,
        0,
        GetWindowLongW(hwnd, GWL_EXSTYLE) as u32,
    );
    let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = mem::zeroed();
    info.cbSize = mem::size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(monitor, &mut info);
    let work = info.rcWork;
    let w = bounds.right - bounds.left;
    let h = bounds.bottom - bounds.top;
    SetWindowPos(
        hwnd,
        0,
        work.left + ((work.right - work.left - w) / 2).max(0),
        work.top + ((work.bottom - work.top - h) / 2).max(0),
        w,
        h,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
}

unsafe fn font() -> HFONT {
    GetStockObject(DEFAULT_GUI_FONT) as HFONT
}

unsafe fn button(hwnd: HWND, id: i32, label: &str, x: i32, y: i32, width: i32) -> HWND {
    let control = create_ctrl(
        hwnd,
        font(),
        "BUTTON",
        label,
        0,
        WS_TABSTOP
            | if id == IDOK {
                BS_DEFPUSHBUTTON as u32
            } else {
                0
            },
        x,
        y,
        width,
        24,
        id,
    );
    advanced_style::button(control, false, id == IDOK);
    control
}

unsafe fn label(hwnd: HWND, id: i32, text: &str, x: i32, y: i32, width: i32) {
    create_ctrl(hwnd, font(), "STATIC", text, 0, 0, x, y, width, 19, id);
}

unsafe fn field(hwnd: HWND, id: i32, caption: &str, value: &str, y: i32, width: i32) -> HWND {
    label(hwnd, -1, caption, 15, y + 3, 40);
    create_ctrl(
        hwnd,
        font(),
        "EDIT",
        value,
        WS_EX_CLIENTEDGE,
        WS_TABSTOP | ES_AUTOHSCROLL as u32,
        58,
        y,
        width,
        22,
        id,
    )
}

unsafe fn populate_manager(hwnd: HWND, manager: &mut Manager) {
    for (index, (title, id)) in [
        ("添加(&A)", ADD),
        ("删除(&D)", DELETE),
        ("修改(&M)", MODIFY),
        ("退出(&X)", IDCANCEL),
    ]
    .into_iter()
    .enumerate()
    {
        button(hwnd, id, title, 14 + index as i32 * 72, 14, 66);
    }
    let filter = create_ctrl(
        hwnd,
        font(),
        "LISTBOX",
        "",
        WS_EX_CLIENTEDGE,
        WS_TABSTOP
            | LBS_NOTIFY as u32
            | LBS_NOINTEGRALHEIGHT as u32
            | LBS_OWNERDRAWFIXED as u32
            | LBS_HASSTRINGS as u32,
        14,
        55,
        104,
        408,
        FILTER,
    );
    SendMessageW(filter, LB_SETITEMHEIGHT, 0, 21);
    for title in FILTERS {
        add_list_item(filter, title);
    }
    SendMessageW(filter, LB_SETCURSEL, 0, 0);
    let list = create_ctrl(
        hwnd,
        font(),
        "SysListView32",
        "",
        WS_EX_CLIENTEDGE,
        WS_TABSTOP | LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS,
        128,
        55,
        434,
        408,
        LIST,
    );
    SendMessageW(
        list,
        LVM_SETEXTENDEDLISTVIEWSTYLE,
        0,
        (LVS_EX_FULLROWSELECT | LVS_EX_GRIDLINES | LVS_EX_CHECKBOXES | LVS_EX_DOUBLEBUFFER)
            as LPARAM,
    );
    for (index, (title, width)) in [("热键", 90), ("链接", 198), ("备注", 136)]
        .into_iter()
        .enumerate()
    {
        let mut text = to_wide(title);
        let mut column: LVCOLUMNW = mem::zeroed();
        column.mask = LVCF_TEXT | LVCF_WIDTH;
        column.cx = width;
        column.pszText = text.as_mut_ptr();
        SendMessageW(
            list,
            LVM_INSERTCOLUMNW,
            index,
            &column as *const _ as LPARAM,
        );
    }
    label(hwnd, STATUS, "", 14, 475, 548);
    refresh(hwnd, manager, None);
}

fn shortcut_info(id: &str) -> Option<(TargetKind, String, String, String, String)> {
    APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let item = guard
            .as_ref()?
            .categories
            .iter()
            .flat_map(|category| &category.groups)
            .flat_map(|group| &group.items)
            .find(|item| item.id == id)?;
        Some((
            item.target_kind,
            item.target_path.clone(),
            item.args.clone(),
            item.work_dir.clone(),
            item.name.clone(),
        ))
    })
}

fn describe(binding: &Binding) -> (usize, String, String) {
    let (kind, link, name) = match &binding.action {
        Action::Shortcut { id } => match shortcut_info(id) {
            Some((kind, target, _, _, name)) => (kind, target, name),
            None => return (7, id.clone(), binding.remark.clone()),
        },
        Action::Target { kind, target, .. } => (*kind, target.clone(), String::new()),
        Action::Text { .. } => return (6, "输出文本".into(), binding.remark.clone()),
    };
    let filter = match kind {
        TargetKind::File => 1,
        TargetKind::Folder => 2,
        TargetKind::System => 3,
        TargetKind::Web => 4,
        TargetKind::Launcher => 5,
    };
    (
        filter,
        link,
        if binding.remark.is_empty() {
            name
        } else {
            binding.remark.clone()
        },
    )
}

unsafe fn refresh(hwnd: HWND, manager: &mut Manager, selected: Option<usize>) {
    let list = GetDlgItem(hwnd, LIST);
    SendMessageW(list, WM_SETREDRAW, 0, 0);
    SendMessageW(list, LVM_DELETEALLITEMS, 0, 0);
    manager.visible.clear();
    for (index, (key, binding)) in manager.config.bindings.iter().enumerate() {
        let (filter, target, remark) = describe(binding);
        if manager.filter != 0 && manager.filter != filter {
            continue;
        }
        let row = manager.visible.len();
        manager.visible.push(index);
        for (column, value) in [key.as_str(), &target, &remark].into_iter().enumerate() {
            let mut text = to_wide(value);
            let mut item: LVITEMW = mem::zeroed();
            item.mask = LVIF_TEXT;
            item.iItem = row as i32;
            item.iSubItem = column as i32;
            item.pszText = text.as_mut_ptr();
            if column == 0 {
                SendMessageW(list, LVM_INSERTITEMW, 0, &item as *const _ as LPARAM);
            } else {
                SendMessageW(list, LVM_SETITEMTEXTW, row, &item as *const _ as LPARAM);
            }
        }
        let mut item: LVITEMW = mem::zeroed();
        item.stateMask = LVIS_STATEIMAGEMASK;
        item.state = if binding.enabled { 2 << 12 } else { 1 << 12 };
        if selected == Some(index) || selected.is_none() && row == 0 {
            item.stateMask |= LVIS_SELECTED | LVIS_FOCUSED;
            item.state |= LVIS_SELECTED | LVIS_FOCUSED;
        }
        SendMessageW(list, LVM_SETITEMSTATE, row, &item as *const _ as LPARAM);
    }
    SendMessageW(list, WM_SETREDRAW, 1, 0);
    InvalidateRect(list, ptr::null(), 1);
    update_status(hwnd, manager);
}

unsafe fn selected(hwnd: HWND, manager: &Manager) -> Option<usize> {
    let row = SendMessageW(
        GetDlgItem(hwnd, LIST),
        LVM_GETNEXTITEM,
        usize::MAX,
        LVNI_SELECTED as LPARAM,
    );
    usize::try_from(row)
        .ok()
        .and_then(|index| manager.visible.get(index).copied())
}

unsafe fn update_status(hwnd: HWND, manager: &Manager) {
    let status = if let Some(index) = selected(hwnd, manager) {
        let (key, binding) = &manager.config.bindings[index];
        if !binding.enabled {
            format!("已停用热键  {key}")
        } else if hotkeys::is_registered(key) {
            format!("已启用热键  {key}")
        } else {
            format!("热键未注册  {key}")
        }
    } else {
        format!("共 {} 个热键", manager.visible.len())
    };
    set_ctrl_text(hwnd, STATUS, &status);
}

unsafe fn save(hwnd: HWND, manager: &mut Manager, next: HotkeyConfig) -> bool {
    if let Err(error) = next.try_save(&manager.base) {
        crate::advanced_runtime::message(hwnd, &format!("保存热键失败：{error}"));
        return false;
    }
    manager.config = next.clone();
    APP_STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                app.hotkey_cfg = next;
            }
        }
    });
    hotkeys::sync(manager.main);
    true
}

unsafe fn edit(hwnd: HWND, manager: &mut Manager, index: Option<usize>, shortcut: Option<&str>) {
    let existing = index.and_then(|index| manager.config.bindings.get(index));
    let mut editor = Editor {
        base: manager.base.clone(),
        key: existing.map_or_else(String::new, |(key, _)| key.clone()),
        action: existing
            .map(|(_, binding)| binding.action.clone())
            .or_else(|| shortcut.map(|id| Action::Shortcut { id: id.into() })),
        remark: existing.map_or_else(String::new, |(_, binding)| binding.remark.clone()),
        enabled: existing.is_none_or(|(_, binding)| binding.enabled),
        initial_link: String::new(),
        initial_args: String::new(),
        initial_start: String::new(),
        used_keys: manager
            .config
            .bindings
            .iter()
            .enumerate()
            .filter(|(row, _)| Some(*row) != index)
            .filter_map(|(_, (key, _))| Key::parse(key))
            .collect(),
    };
    if let Some(action) = &editor.action {
        match action {
            Action::Shortcut { id } => {
                if let Some((_, target, args, work_dir, _)) = shortcut_info(id) {
                    (
                        editor.initial_link,
                        editor.initial_args,
                        editor.initial_start,
                    ) = (target, args, work_dir);
                } else {
                    editor.initial_link = id.clone();
                }
            }
            Action::Target {
                target,
                args,
                work_dir,
                ..
            } => {
                (
                    editor.initial_link,
                    editor.initial_args,
                    editor.initial_start,
                ) = (target.clone(), args.clone(), work_dir.clone())
            }
            Action::Text { .. } => editor.initial_link = action.caption().into(),
        }
    }
    hotkeys::clear(manager.main);
    let result = show(hwnd, "设置热键", (442, 224), Dialog::Editor(editor));
    let Some(Dialog::Editor(editor)) = result else {
        hotkeys::sync(manager.main);
        return;
    };
    let Some(action) = editor.action else {
        hotkeys::sync(manager.main);
        return;
    };
    let mut next = manager.config.clone();
    let row = index.unwrap_or(next.bindings.len());
    let entry = (
        editor.key,
        Binding {
            enabled: editor.enabled,
            action,
            remark: editor.remark,
        },
    );
    if index.is_some() {
        next.bindings[row] = entry;
    } else {
        next.bindings.push(entry);
    }
    if save(hwnd, manager, next) {
        if GetDlgItem(hwnd, LIST) != 0 {
            refresh(hwnd, manager, Some(row));
        }
    } else {
        hotkeys::sync(manager.main);
    }
}

unsafe fn populate_editor(hwnd: HWND, editor: &Editor) {
    let key = field(hwnd, KEY, "热键", &editor.key, 23, 128);
    SetWindowSubclass(key, Some(capture_key), 1, 0);
    let tooltip_class = to_wide("tooltips_class32");
    let tooltip = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        tooltip_class.as_ptr(),
        ptr::null(),
        WS_POPUP | TTS_ALWAYSTIP,
        0,
        0,
        0,
        0,
        hwnd,
        0,
        GetModuleHandleW(ptr::null()),
        ptr::null(),
    );
    static TITLES: OnceLock<[Vec<u16>; 4]> = OnceLock::new();
    let titles = TITLES.get_or_init(|| ICONS.map(|(_, _, title)| to_wide(title)));
    for (index, &(id, _, title)) in ICONS.iter().enumerate() {
        let control = create_ctrl(
            hwnd,
            font(),
            "BUTTON",
            title,
            0,
            WS_TABSTOP | BS_OWNERDRAW as u32,
            318 + index as i32 * 27,
            22,
            25,
            24,
            id,
        );
        if tooltip != 0 && control != 0 {
            let mut tool: TTTOOLINFOW = mem::zeroed();
            tool.cbSize = mem::size_of::<TTTOOLINFOW>() as u32;
            tool.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
            tool.hwnd = hwnd;
            tool.uId = control as usize;
            tool.lpszText = titles[index].as_ptr().cast_mut();
            SendMessageW(tooltip, TTM_ADDTOOLW, 0, &tool as *const _ as LPARAM);
        }
    }
    field(hwnd, LINK, "链接", &editor.initial_link, 55, 320);
    button(hwnd, CONVERT, "转换", 383, 54, 44);
    field(hwnd, ARGS, "参数", &editor.initial_args, 87, 369);
    field(hwnd, START, "起始", &editor.initial_start, 119, 369);
    field(hwnd, REMARK, "备注", &editor.remark, 151, 369);
    button(hwnd, IDOK, "确定", 272, 190, 73);
    button(hwnd, IDCANCEL, "取消", 354, 190, 73);
    SetFocus(key);
}

unsafe extern "system" fn capture_key(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> isize {
    if message == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(capture_key), 1);
    }
    if matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN | WM_GETDLGCODE) {
        let (vk, queried) = if message == WM_GETDLGCODE {
            if lparam == 0 {
                return DefSubclassProc(hwnd, message, wparam, lparam);
            }
            let msg = &*(lparam as *const MSG);
            (msg.wParam as u16, msg.message)
        } else {
            (wparam as u16, message)
        };
        let mut modifier = 0;
        for (keys, bit) in [
            ([VK_CONTROL, VK_CONTROL], MOD_CONTROL),
            ([VK_MENU, VK_MENU], MOD_ALT),
            ([VK_SHIFT, VK_SHIFT], MOD_SHIFT),
            ([VK_LWIN, VK_RWIN], MOD_WIN),
        ] {
            if keys.into_iter().any(|key| GetKeyState(key as i32) < 0) {
                modifier |= bit;
            }
        }
        let name = (Key {
            modifiers: modifier,
            vk,
        })
        .name();
        let capture = name.is_some()
            && (modifier & !MOD_SHIFT != 0
                || !matches!(vk, VK_ESCAPE | VK_RETURN | VK_TAB | VK_BACK | VK_DELETE));
        if capture && matches!(queried, WM_KEYDOWN | WM_SYSKEYDOWN) {
            if message == WM_GETDLGCODE {
                return DLGC_WANTALLKEYS as isize;
            }
            SetWindowTextW(hwnd, to_wide(&name.unwrap_or_default()).as_ptr());
            return 0;
        }
    }
    if matches!(message, WM_CHAR | WM_SYSCHAR) {
        return 0;
    }
    if message == WM_KEYDOWN && matches!(wparam as u16, VK_BACK | VK_DELETE) {
        SetWindowTextW(hwnd, to_wide("").as_ptr());
        return 0;
    }
    DefSubclassProc(hwnd, message, wparam, lparam)
}

unsafe fn set_target(hwnd: HWND, editor: &mut Editor, action: Action, args: &str, start: &str) {
    editor.initial_link = action.caption().into();
    editor.initial_args = args.into();
    editor.initial_start = start.into();
    set_ctrl_text(hwnd, LINK, &editor.initial_link);
    set_ctrl_text(hwnd, ARGS, args);
    set_ctrl_text(hwnd, START, start);
    editor.action = Some(action);
}

unsafe fn editor_command(hwnd: HWND, editor: &mut Editor, id: i32) {
    match id {
        FILE | FOLDER => {
            let path = if id == FILE {
                crate::pick_file_dialog(hwnd, "添加文件", &[("所有文件", "*.*")])
            } else {
                crate::pick_folder_dialog(hwnd, "添加文件夹")
            };
            if let Some(path) = path {
                let kind = if id == FILE {
                    TargetKind::File
                } else {
                    TargetKind::Folder
                };
                let relative = APP_STATE
                    .with(|cell| {
                        cell.try_borrow().ok().and_then(|guard| {
                            guard.as_ref().map(|app| {
                                app.user_cfg
                                    .advanced
                                    .enabled(crate::advanced::Key::RelativePaths)
                            })
                        })
                    })
                    .unwrap_or(false);
                let to_display = |path: &std::path::Path| {
                    let expanded = path.to_string_lossy();
                    if relative {
                        targets::convert_path(&editor.base, &expanded, PathMode::Relative)
                            .unwrap_or_else(|| expanded.into_owned())
                    } else {
                        expanded.into_owned()
                    }
                };
                let start = if id == FILE {
                    path.parent().map(to_display).unwrap_or_default()
                } else {
                    String::new()
                };
                let args = get_ctrl_text(hwnd, ARGS);
                let action = Action::target(kind, to_display(&path), args.clone(), start.clone());
                set_target(hwnd, editor, action, &args, &start);
            }
        }
        SYSTEM => {
            if let Some(choice) = choose_system(hwnd) {
                let action = Action::target(
                    choice.kind,
                    choice.target.into(),
                    choice.args.into(),
                    String::new(),
                );
                set_target(hwnd, editor, action, choice.args, "");
                if get_ctrl_text(hwnd, REMARK).is_empty() {
                    set_ctrl_text(hwnd, REMARK, choice.label);
                }
            }
        }
        TEXT => {
            let existing = match &editor.action {
                Some(Action::Text { encrypted }) => match hotkeys::decrypt(encrypted) {
                    Some(value) => value,
                    None => {
                        crate::advanced_runtime::message(
                            hwnd,
                            "当前 Windows 账户无法读取原有文本。",
                        );
                        String::new()
                    }
                },
                _ => String::new(),
            };
            if let Some(Dialog::Text(value)) =
                show(hwnd, "热键输出文本", (362, 367), Dialog::Text(existing))
            {
                let Some(encrypted) = hotkeys::encrypt(&value) else {
                    crate::advanced_runtime::message(hwnd, "无法加密文本。");
                    return;
                };
                set_target(hwnd, editor, Action::Text { encrypted }, "", "");
            }
        }
        CONVERT => {
            let link = get_ctrl_text(hwnd, LINK);
            if link == editor.initial_link && matches!(&editor.action, Some(Action::Text { .. })) {
                return;
            }
            let kind = editor_kind(editor, &link);
            if !matches!(kind, TargetKind::File | TargetKind::Folder) {
                return;
            }
            let choices = [("转换成相对路径(&X)", false), ("转换成绝对路径(&J)", false)];
            let Some(selection) = crate::popup::choose(hwnd, GetDlgItem(hwnd, id), &choices, 188)
            else {
                return;
            };
            let mode = if selection == 0 {
                PathMode::Relative
            } else {
                PathMode::Absolute
            };
            let start = get_ctrl_text(hwnd, START);
            let converted = targets::convert_path(&editor.base, link.trim(), mode).zip(
                if start.trim().is_empty() {
                    Some(String::new())
                } else {
                    targets::convert_path(&editor.base, start.trim(), mode)
                },
            );
            if let Some((link, start)) = converted {
                let args = get_ctrl_text(hwnd, ARGS);
                set_target(
                    hwnd,
                    editor,
                    Action::target(kind, link, args.clone(), start.clone()),
                    &args,
                    &start,
                );
            } else {
                crate::advanced_runtime::message(
                    hwnd,
                    "无法转换此路径。相对路径要求与 KStart 位于同一驱动器或共享位置。",
                );
            }
        }
        IDOK => {
            let key = get_ctrl_text(hwnd, KEY);
            let Some(key) = Key::parse(&key) else {
                crate::advanced_runtime::message(hwnd, "请按下有效的热键组合。");
                return;
            };
            if editor.used_keys.contains(&key) {
                crate::advanced_runtime::message(hwnd, "这个热键已经存在。");
                return;
            }
            let Some(key) = key.name() else {
                return;
            };
            let link = get_ctrl_text(hwnd, LINK);
            let args = get_ctrl_text(hwnd, ARGS);
            let start = get_ctrl_text(hwnd, START);
            if link.trim().is_empty() {
                crate::advanced_runtime::message(hwnd, "请输入链接或选择热键目标。");
                return;
            }
            let unchanged_text =
                link == editor.initial_link && matches!(&editor.action, Some(Action::Text { .. }));
            if !unchanged_text
                && (link != editor.initial_link
                    || args != editor.initial_args
                    || start != editor.initial_start
                    || editor.action.is_none())
            {
                let kind = editor_kind(editor, &link);
                editor.action = Some(Action::target(
                    kind,
                    link.trim().into(),
                    args,
                    start.trim().into(),
                ));
            }
            editor.key = key;
            editor.remark = get_ctrl_text(hwnd, REMARK);
            EndDialog(hwnd, IDOK as isize);
        }
        _ => {}
    }
}

fn editor_kind(editor: &Editor, path: &str) -> TargetKind {
    if path == editor.initial_link {
        if let Some(Action::Target { kind, .. }) = &editor.action {
            return *kind;
        }
        if let Some(Action::Shortcut { id }) = &editor.action {
            if let Some((kind, ..)) = shortcut_info(id) {
                return kind;
            }
        }
    }
    TargetKind::infer(&editor.base, path.trim())
}

unsafe fn populate_text(hwnd: HWND, body: &str) {
    label(hwnd, -1, "以下文本将加密存储", 16, 26, 198);
    button(hwnd, CONTROLS, "添加控制字符", 233, 21, 111);
    let edit = create_ctrl(
        hwnd,
        font(),
        "EDIT",
        body,
        WS_EX_CLIENTEDGE,
        WS_TABSTOP
            | WS_VSCROLL
            | ES_MULTILINE as u32
            | ES_AUTOVSCROLL as u32
            | ES_WANTRETURN as u32,
        16,
        56,
        328,
        257,
        TEXT_BODY,
    );
    SendMessageW(edit, EM_SETLIMITTEXT, 8192, 0);
    button(hwnd, IDOK, "确定", 190, 332, 73);
    button(hwnd, IDCANCEL, "取消", 271, 332, 73);
    SetFocus(edit);
}

unsafe fn text_command(hwnd: HWND, body: &mut String, id: i32) {
    if id == CONTROLS {
        let choices: Vec<_> = hotkeys::CONTROLS
            .iter()
            .enumerate()
            .map(|(index, (label, ..))| (*label, index == 4))
            .collect();
        if let Some(index) = crate::popup::choose(hwnd, GetDlgItem(hwnd, id), &choices, 138) {
            let edit = GetDlgItem(hwnd, TEXT_BODY);
            let value = to_wide(&format!("{{{}}}", hotkeys::CONTROLS[index].1));
            SendMessageW(edit, EM_REPLACESEL, 1, value.as_ptr() as LPARAM);
            SetFocus(edit);
        }
    } else if id == IDOK {
        let value = get_ctrl_text(hwnd, TEXT_BODY);
        if value.is_empty() {
            crate::advanced_runtime::message(hwnd, "请输入要输出的文本。");
        } else {
            *body = value;
            EndDialog(hwnd, IDOK as isize);
        }
    }
}

#[derive(Clone, Copy)]
pub struct Choice {
    pub label: &'static str,
    pub target: &'static str,
    pub args: &'static str,
    pub kind: TargetKind,
}

const TABS: &[&str] = &[
    "系统链接",
    "系统操作",
    "音量控制",
    "控制面板",
    "拨号",
    "音速启动",
];
const SYSTEM_GROUPS: &[&[(&str, &str, &str)]] = &[
    &[
        ("我的电脑", "shell:MyComputerFolder", ""),
        ("我的文档", "shell:Personal", ""),
        ("控制面板", "control.exe", ""),
        ("回收站", "shell:RecycleBinFolder", ""),
        ("网上邻居", "shell:NetworkPlacesFolder", ""),
        ("资源管理器", "explorer.exe", ""),
        ("打印机", "shell:PrintersFolder", ""),
        ("注册表编辑器", "regedit.exe", ""),
        ("命令提示符", "cmd.exe", ""),
        ("文件夹选项", "control.exe", "folders"),
    ],
    &[
        ("关机", "kstart:shutdown", ""),
        ("重启", "kstart:restart", ""),
        ("注销", "kstart:logoff", ""),
        ("锁定计算机", "kstart:lock", ""),
        ("睡眠", "kstart:sleep", ""),
        ("休眠", "kstart:hibernate", ""),
        ("断开 Wi-Fi", "kstart:disconnect", ""),
        ("屏幕保护", "kstart:screensaver", ""),
        ("打开光驱", "kstart:cd-open", ""),
        ("关闭光驱", "kstart:cd-close", ""),
        ("关闭显示器", "kstart:display-off", ""),
        ("清空回收站", "kstart:recycle", ""),
    ],
    &[
        ("增大音量", "kstart:volume-up", ""),
        ("减小音量", "kstart:volume-down", ""),
        ("静音/取消静音", "kstart:volume-mute", ""),
        ("声音设置", "control.exe", "mmsys.cpl"),
    ],
    &[
        ("控制面板", "control.exe", ""),
        ("网络连接", "control.exe", "ncpa.cpl"),
        ("程序和功能", "control.exe", "appwiz.cpl"),
        ("日期和时间", "control.exe", "timedate.cpl"),
        ("鼠标", "control.exe", "main.cpl"),
        ("电源选项", "control.exe", "powercfg.cpl"),
    ],
    &[
        ("网络连接", "control.exe", "ncpa.cpl"),
        ("拨号连接", "rasphone.exe", ""),
    ],
    &[
        ("激活音速启动", "kstart:app-show", ""),
        ("隐藏音速启动", "kstart:app-hide", ""),
        ("激活/隐藏音速启动", "/AH/", ""),
        ("退出音速启动", "kstart:app-exit", ""),
        ("打开高级设置", "kstart:app-advanced", ""),
        ("打开热键管理", "kstart:app-hotkeys", ""),
        ("打开搜索窗口", "/Search/", ""),
    ],
];

pub unsafe fn choose_system(owner: HWND) -> Option<Choice> {
    match show(
        owner,
        "添加系统功能",
        (392, 426),
        Dialog::System {
            tab: 0,
            selected: None,
        },
    ) {
        Some(Dialog::System { selected, .. }) => selected,
        _ => None,
    }
}

unsafe fn add_list_item(list: HWND, title: &str) {
    SendMessageW(list, LB_ADDSTRING, 0, to_wide(title).as_ptr() as LPARAM);
}

unsafe fn populate_system(hwnd: HWND) {
    let tab = create_ctrl(
        hwnd,
        font(),
        "SysTabControl32",
        "",
        0,
        WS_TABSTOP,
        14,
        14,
        364,
        355,
        TAB,
    );
    for (index, title) in TABS.iter().enumerate() {
        let mut text = to_wide(title);
        let mut item: TCITEMW = mem::zeroed();
        item.mask = TCIF_TEXT;
        item.pszText = text.as_mut_ptr();
        SendMessageW(tab, TCM_INSERTITEMW, index, &item as *const _ as LPARAM);
    }
    for (index, id) in [SYSTEM_LEFT, SYSTEM_RIGHT].into_iter().enumerate() {
        let list = create_ctrl(
            hwnd,
            font(),
            "LISTBOX",
            "",
            WS_EX_CLIENTEDGE,
            WS_TABSTOP
                | WS_VSCROLL
                | LBS_NOTIFY as u32
                | LBS_OWNERDRAWFIXED as u32
                | LBS_HASSTRINGS as u32
                | LBS_NOINTEGRALHEIGHT as u32,
            22 + index as i32 * 176,
            42,
            174,
            316,
            id,
        );
        SendMessageW(list, LB_SETITEMHEIGHT, 0, 23);
        SetWindowSubclass(list, Some(system_list_key), 1, 0);
    }
    button(hwnd, IDCANCEL, "关闭", 304, 386, 73);
    refresh_system(hwnd, 0);
}

unsafe extern "system" fn system_list_key(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> isize {
    let enter = wparam as u16 == VK_RETURN;
    if message == WM_GETDLGCODE && enter {
        return DLGC_WANTALLKEYS as isize;
    }
    if message == WM_KEYDOWN && enter {
        let parent = GetParent(hwnd);
        PostMessageW(
            parent,
            WM_COMMAND,
            GetDlgCtrlID(hwnd) as usize | ((LBN_DBLCLK as usize) << 16),
            hwnd as LPARAM,
        );
        return 0;
    }
    if message == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(system_list_key), 1);
    }
    DefSubclassProc(hwnd, message, wparam, lparam)
}

unsafe fn refresh_system(hwnd: HWND, tab: usize) {
    for (column, id) in [SYSTEM_LEFT, SYSTEM_RIGHT].into_iter().enumerate() {
        let list = GetDlgItem(hwnd, id);
        SendMessageW(list, LB_RESETCONTENT, 0, 0);
        for &(title, _, _) in SYSTEM_GROUPS[tab].iter().skip(column).step_by(2) {
            add_list_item(list, title);
        }
    }
}

unsafe fn choose_system_row(hwnd: HWND, tab: usize, id: i32) -> Option<Choice> {
    let row = SendMessageW(GetDlgItem(hwnd, id), LB_GETCURSEL, 0, 0);
    let index = usize::try_from(row).ok()? * 2 + usize::from(id == SYSTEM_RIGHT);
    let &(label, target, args) = SYSTEM_GROUPS[tab].get(index)?;
    let kind = if tab == 5 {
        TargetKind::Launcher
    } else {
        TargetKind::System
    };
    Some(Choice {
        label,
        target,
        args,
        kind,
    })
}

fn icons() -> &'static [Option<RgbaSurface>; 4] {
    static SURFACES: OnceLock<[Option<RgbaSurface>; 4]> = OnceLock::new();
    SURFACES.get_or_init(|| {
        ICONS.map(|(_, bytes, _)| {
            let image = image::load_from_memory(bytes).ok()?.to_rgba8();
            Some(RgbaSurface {
                width: image.width(),
                height: image.height(),
                pixels: image.pixels().map(|p| p.0).collect(),
            })
        })
    })
}

unsafe fn draw_item(draw: &DRAWITEMSTRUCT) -> bool {
    let id = draw.CtlID as i32;
    if !(FILE..=TEXT).contains(&id) && !matches!(id, FILTER | SYSTEM_LEFT | SYSTEM_RIGHT) {
        return false;
    }
    let button = (FILE..=TEXT).contains(&id);
    let selected = draw.itemState & ODS_SELECTED != 0;
    let color = if selected {
        COLOR_HIGHLIGHT
    } else if button {
        COLOR_BTNFACE
    } else {
        COLOR_WINDOW
    };
    FillRect(draw.hDC, &draw.rcItem, GetSysColorBrush(color));
    let [r, g, b, _] = GetSysColor(color).to_le_bytes();
    let rect = draw.rcItem;
    let icon = if button {
        (id - FILE) as usize
    } else if id == FILTER {
        1
    } else {
        2
    };
    if let Some(surface) = &icons()[icon] {
        let x = if button {
            rect.left + (rect.right - rect.left - 16) / 2
        } else {
            rect.left + 4
        };
        blit_surface_on_bg(draw.hDC, surface, x, rect.top + 4, 16, 16, [r, g, b]);
    }
    if !button && draw.itemID != u32::MAX {
        let mut name = [0u16; 80];
        let len = SendMessageW(
            draw.hwndItem,
            LB_GETTEXT,
            draw.itemID as usize,
            name.as_mut_ptr() as LPARAM,
        );
        if (1..80).contains(&len) {
            let mut area = RECT {
                left: rect.left + 24,
                ..rect
            };
            let old_font = SelectObject(draw.hDC, font() as _);
            let old_mode = SetBkMode(draw.hDC, TRANSPARENT as i32);
            let old_color = SetTextColor(
                draw.hDC,
                GetSysColor(if selected {
                    COLOR_HIGHLIGHTTEXT
                } else {
                    COLOR_WINDOWTEXT
                }),
            );
            DrawTextW(
                draw.hDC,
                name.as_mut_ptr(),
                len as i32,
                &mut area,
                DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
            SetTextColor(draw.hDC, old_color);
            SetBkMode(draw.hDC, old_mode);
            SelectObject(draw.hDC, old_font);
        }
    }
    if draw.itemState & ODS_FOCUS != 0 {
        DrawFocusRect(draw.hDC, &rect);
    }
    true
}

unsafe extern "system" fn dialog_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    if message == WM_INITDIALOG {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam);
    }
    if message == WM_DRAWITEM {
        return (lparam as *const DRAWITEMSTRUCT)
            .as_ref()
            .is_some_and(|draw| draw_item(draw)) as isize;
    }
    if message == WM_CLOSE || message == WM_COMMAND && wparam as u16 as i32 == IDCANCEL {
        EndDialog(hwnd, IDCANCEL as isize);
        return 1;
    }
    if !matches!(message, WM_INITDIALOG | WM_COMMAND | WM_NOTIFY) {
        return 0;
    }
    let Some(state) =
        (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<(Dialog, (i32, i32))>).as_ref()
    else {
        return 0;
    };
    let Ok(mut state) = state.try_borrow_mut() else {
        return 0;
    };
    let (dialog, size) = &mut *state;
    if message == WM_INITDIALOG {
        center(hwnd, *size);
        return match dialog {
            Dialog::Manager(manager) => {
                populate_manager(hwnd, manager);
                1
            }
            Dialog::Editor(editor) => {
                populate_editor(hwnd, editor);
                0
            }
            Dialog::Text(body) => {
                populate_text(hwnd, body);
                0
            }
            Dialog::System { .. } => {
                populate_system(hwnd);
                1
            }
        };
    }
    let id = wparam as u16 as i32;
    let notification = (wparam >> 16) as u16 as u32;
    if message == WM_COMMAND {
        match dialog {
            Dialog::Manager(manager) => match id {
                ADD => edit(hwnd, manager, None, None),
                MODIFY => {
                    if let Some(row) = selected(hwnd, manager) {
                        edit(hwnd, manager, Some(row), None);
                    }
                }
                DELETE => {
                    if let Some(row) = selected(hwnd, manager) {
                        let mut next = manager.config.clone();
                        next.bindings.remove(row);
                        if save(hwnd, manager, next) {
                            refresh(hwnd, manager, None);
                        }
                    }
                }
                FILTER if notification == LBN_SELCHANGE => {
                    let index = SendMessageW(GetDlgItem(hwnd, FILTER), LB_GETCURSEL, 0, 0);
                    manager.filter = index.max(0) as usize;
                    refresh(hwnd, manager, None);
                }
                _ => {}
            },
            Dialog::Editor(editor) => editor_command(hwnd, editor, id),
            Dialog::Text(body) => text_command(hwnd, body, id),
            Dialog::System { tab, selected } => {
                let id = if id == IDOK {
                    GetDlgCtrlID(GetFocus())
                } else {
                    id
                };
                if matches!(id, SYSTEM_LEFT | SYSTEM_RIGHT)
                    && (notification == LBN_DBLCLK || wparam as i32 == IDOK)
                {
                    *selected = choose_system_row(hwnd, *tab, id);
                    if selected.is_some() {
                        EndDialog(hwnd, IDOK as isize);
                    }
                }
            }
        }
        return 1;
    }
    let Some(header) = (lparam as *const NMHDR).as_ref() else {
        return 0;
    };
    match dialog {
        Dialog::Manager(manager) if header.idFrom == LIST as usize => {
            if header.code == NM_DBLCLK {
                if let Some(row) = selected(hwnd, manager) {
                    edit(hwnd, manager, Some(row), None);
                }
            } else if header.code == LVN_ITEMCHANGED {
                let changed = &*(lparam as *const NMLISTVIEW);
                let from = changed.uOldState & LVIS_STATEIMAGEMASK;
                let to = changed.uNewState & LVIS_STATEIMAGEMASK;
                if changed.uChanged & LVIF_STATE != 0 && from != 0 && to != 0 && from != to {
                    if let Some(&row) = manager.visible.get(changed.iItem as usize) {
                        let mut next = manager.config.clone();
                        next.bindings[row].1.enabled = to == 2 << 12;
                        if !save(hwnd, manager, next) {
                            refresh(hwnd, manager, Some(row));
                        }
                    }
                }
                update_status(hwnd, manager);
            }
        }
        Dialog::System { tab, .. }
            if header.idFrom == TAB as usize && header.code == TCN_SELCHANGE =>
        {
            *tab = SendMessageW(GetDlgItem(hwnd, TAB), TCM_GETCURSEL, 0, 0)
                .clamp(0, (TABS.len() - 1) as isize) as usize;
            refresh_system(hwnd, *tab);
        }
        _ => {}
    }
    0
}
