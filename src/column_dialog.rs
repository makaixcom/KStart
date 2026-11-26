//! 栏目新建、右键菜单的排序、标题编辑和批量导入导出窗口。
use crate::{
    advanced::AdvancedSettings,
    model::{self, ShortcutGroup, ShortcutItem, ShortcutSortKey},
    native_dialogs::{create_classic_ctrl as ctrl, get_ctrl_text, set_ctrl_text},
    shortcut_transfer::{self, ScanEvent},
    to_wide, APP_STATE,
};
use std::{
    cell::RefCell,
    collections::HashSet,
    mem,
    path::PathBuf,
    ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    Storage::FileSystem::GetLogicalDrives,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::*,
        Input::KeyboardAndMouse::{EnableWindow, SetFocus},
        WindowsAndMessaging::*,
    },
};

const TAB: i32 = 1800;
const LIST: i32 = 1801;
const DELETE: i32 = 1802;
const PATH: i32 = 1803;
const ICON: i32 = 1804;
const MINIMIZE: i32 = 1805;
const ALL: i32 = 1806;
const NONE: i32 = 1807;
const INVERT: i32 = 1808;
const REMOVE: i32 = 1809;
const RANGE: i32 = 1810;
const PATTERN: i32 = 1811;
const RECURSIVE: i32 = 1812;
const START: i32 = 1813;
const STATUS: i32 = 1814;
const SEARCH_ICON: i32 = 1815;
const TITLE: i32 = 1820;
const BEFORE: i32 = 1821;
const AFTER: i32 = 1822;
const SORT_TITLE: i32 = 1830;
const SORT_PATH: i32 = 1831;
const SORT_TIME: i32 = 1832;
const ASCENDING: i32 = 1833;
const DESCENDING: i32 = 1834;
const MIN_YES: i32 = 1835;
const MIN_NO: i32 = 1836;
const NORMAL_COLUMN: i32 = 1840;
const FOLDER_COLUMN: i32 = 1841;

#[derive(Clone, Copy)]
pub enum Action {
    Sort,
    Rename,
    Batch,
}

struct Manager {
    column: String,
    base: PathBuf,
    settings: AdvancedSettings,
    items: Vec<ShortcutItem>,
    tab: usize,
    selection: [Vec<usize>; 3],
    paths: Vec<PathBuf>,
    receiver: Option<mpsc::Receiver<ScanEvent>>,
    cancel: Arc<AtomicBool>,
}

enum Dialog {
    Sort {
        key: ShortcutSortKey,
        descending: bool,
    },
    Rename(String),
    Create(String),
    Paths {
        before: String,
        after: String,
    },
    Minimize(bool),
    Batch(Manager),
}

struct State {
    dialog: RefCell<Dialog>,
    size: (i32, i32),
    font: HFONT,
}

/// 普通栏目仅在确认后创建；对话框期间不占用全局状态借用。
pub unsafe fn create_column(owner: HWND, category: usize) {
    let category_id = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        Some(guard.as_ref()?.categories.get(category)?.id.clone())
    });
    let Some(category_id) = category_id else {
        return;
    };
    let Some(Dialog::Create(title)) = show(
        owner,
        "添加栏目",
        (372, 228),
        Dialog::Create("新建栏目".into()),
    ) else {
        return;
    };
    let created = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        let category = app
            .categories
            .iter()
            .position(|cat| cat.id == category_id)?;
        app.append_column(category, title)?;
        app.active_category = category;
        app.selected_shortcuts.clear();
        app.dragging_shortcut = None;
        app.marquee_rect = None;
        app.hover_target = crate::renderer::HitTarget::None;
        app.down_target = crate::renderer::HitTarget::None;
        app.advanced_window.shortcut_scroll = 0;
        app.persist_data();
        Some(())
    });
    if created.is_some() {
        InvalidateRect(owner, ptr::null(), 0);
    } else {
        notify(owner, "目标类别已不存在，栏目未添加。");
    }
}

/// 只在 APP_STATE 借用释放后进入窗口的模态消息循环。
pub unsafe fn open(owner: HWND, category: usize, group: usize, action: Action) {
    let snapshot = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        let column = app.categories.get(category)?.groups.get(group)?;
        Some((
            column.id.clone(),
            column.title.clone(),
            column.items.clone(),
            app.base_dir.clone(),
            app.user_cfg.advanced.clone(),
        ))
    });
    let Some((column, title, items, base, settings)) = snapshot else {
        return;
    };
    match action {
        Action::Sort => {
            if let Some(Dialog::Sort { key, descending }) = show(
                owner,
                "快捷方式排序",
                (284, 202),
                Dialog::Sort {
                    key: ShortcutSortKey::Title,
                    descending: false,
                },
            ) {
                mutate(&column, |group| {
                    model::sort_shortcuts(&mut group.items, key, descending)
                });
            }
        }
        Action::Rename => {
            if let Some(Dialog::Rename(title)) =
                show(owner, "修改标题", (324, 112), Dialog::Rename(title))
            {
                mutate(&column, |group| group.title = title);
            }
        }
        Action::Batch => {
            show(
                owner,
                "批量操作/导出/导入",
                (584, 466),
                Dialog::Batch(Manager {
                    column,
                    base,
                    settings,
                    items,
                    tab: 0,
                    selection: Default::default(),
                    paths: Vec::new(),
                    receiver: None,
                    cancel: Arc::new(AtomicBool::new(false)),
                }),
            );
        }
    }
}

/// 通过稳定栏目 ID 提交一次修改；弹窗期间不保存全局状态借用。
fn mutate(column: &str, action: impl FnOnce(&mut ShortcutGroup)) -> Option<Vec<ShortcutItem>> {
    let result = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        let group = app
            .categories
            .iter_mut()
            .flat_map(|cat| &mut cat.groups)
            .find(|group| group.id == column)?;
        action(group);
        let items = group.items.clone();
        app.selected_shortcuts.clear();
        app.persist_data();
        Some((items, app.main_hwnd))
    });
    if let Some((_, main)) = &result {
        unsafe {
            InvalidateRect(*main, ptr::null(), 0);
        }
    }
    result.map(|(items, _)| items)
}

unsafe fn show(owner: HWND, title: &str, size: (i32, i32), dialog: Dialog) -> Option<Dialog> {
    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_MODALFRAME as u32;
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
        OUT_DEFAULT_PRECIS as u32,
        0,
        DEFAULT_QUALITY as u32,
        0,
        to_wide("宋体").as_ptr(),
    );
    let state = State {
        dialog: RefCell::new(dialog),
        size,
        font,
    };
    let result = DialogBoxIndirectParamW(
        GetModuleHandleW(ptr::null()),
        template.as_ptr().cast(),
        owner,
        Some(proc),
        &state as *const _ as LPARAM,
    );
    if let Ok(state) = state.dialog.try_borrow() {
        if let Dialog::Batch(manager) = &*state {
            manager.cancel.store(true, Ordering::Relaxed);
        }
    }
    if font != 0 {
        DeleteObject(font);
    }
    if result == IDOK as isize {
        Some(state.dialog.into_inner())
    } else {
        None
    }
}

unsafe fn state(hwnd: HWND) -> Option<&'static State> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State).as_ref()
}

unsafe fn button(hwnd: HWND, font: HFONT, title: &str, x: i32, y: i32, w: i32, h: i32, id: i32) {
    ctrl(
        hwnd,
        font,
        "BUTTON",
        title,
        0,
        WS_TABSTOP
            | if id == IDOK {
                BS_DEFPUSHBUTTON as u32
            } else {
                0
            },
        x,
        y,
        w,
        h,
        id,
    );
}
unsafe fn label(hwnd: HWND, font: HFONT, title: &str, x: i32, y: i32, w: i32, id: i32) {
    ctrl(hwnd, font, "STATIC", title, 0, 0, x, y, w, 18, id);
}
unsafe fn radio(
    hwnd: HWND,
    font: HFONT,
    title: &str,
    x: i32,
    y: i32,
    w: i32,
    id: i32,
    group: bool,
    checked: bool,
) {
    let control = ctrl(
        hwnd,
        font,
        "BUTTON",
        title,
        0,
        WS_TABSTOP | BS_AUTORADIOBUTTON as u32 | if group { WS_GROUP } else { 0 },
        x,
        y,
        w,
        18,
        id,
    );
    SendMessageW(control, BM_SETCHECK, checked as usize, 0);
}

unsafe fn populate(hwnd: HWND, state: &State) {
    let font = state.font;
    {
        let Ok(dialog) = state.dialog.try_borrow() else {
            return;
        };
        match &*dialog {
            Dialog::Sort { .. } => {
                ctrl(
                    hwnd,
                    font,
                    "BUTTON",
                    "依据",
                    0,
                    BS_GROUPBOX as u32,
                    20,
                    20,
                    248,
                    52,
                    -1,
                );
                radio(hwnd, font, "标题", 46, 38, 62, SORT_TITLE, true, true);
                radio(hwnd, font, "路径", 118, 38, 62, SORT_PATH, false, false);
                radio(hwnd, font, "添加时间", 190, 38, 74, SORT_TIME, false, false);
                ctrl(
                    hwnd,
                    font,
                    "BUTTON",
                    "方向",
                    0,
                    BS_GROUPBOX as u32,
                    20,
                    90,
                    248,
                    52,
                    -1,
                );
                radio(hwnd, font, "升序", 68, 108, 70, ASCENDING, true, true);
                radio(hwnd, font, "降序", 156, 108, 70, DESCENDING, false, false);
                button(hwnd, font, "确定", 114, 168, 72, 21, IDOK);
                button(hwnd, font, "取消", 194, 168, 72, 21, IDCANCEL);
            }
            Dialog::Create(value) => {
                radio(
                    hwnd,
                    font,
                    "普通栏目",
                    26,
                    24,
                    98,
                    NORMAL_COLUMN,
                    true,
                    true,
                );
                radio(
                    hwnd,
                    font,
                    "文件夹/网址栏目",
                    132,
                    24,
                    170,
                    FOLDER_COLUMN,
                    false,
                    false,
                );
                EnableWindow(GetDlgItem(hwnd, FOLDER_COLUMN), 0);
                label(hwnd, font, "栏目标题：", 26, 66, 300, -1);
                ctrl(
                    hwnd,
                    font,
                    "EDIT",
                    value,
                    WS_EX_CLIENTEDGE,
                    WS_TABSTOP | ES_AUTOHSCROLL as u32,
                    42,
                    90,
                    290,
                    20,
                    TITLE,
                );
                button(hwnd, font, "确定", 202, 194, 72, 21, IDOK);
                button(hwnd, font, "取消", 282, 194, 72, 21, IDCANCEL);
            }
            Dialog::Rename(value) => {
                label(hwnd, font, "请输入新的栏目标题:", 14, 14, 290, -1);
                ctrl(
                    hwnd,
                    font,
                    "EDIT",
                    value,
                    WS_EX_CLIENTEDGE,
                    WS_TABSTOP | ES_AUTOHSCROLL as u32,
                    16,
                    46,
                    294,
                    20,
                    TITLE,
                );
                button(hwnd, font, "确定", 160, 84, 72, 21, IDOK);
                button(hwnd, font, "取消", 240, 84, 72, 21, IDCANCEL);
            }
            Dialog::Paths { before, after } => {
                label(hwnd, font, "原目录:", 16, 19, 68, -1);
                ctrl(
                    hwnd,
                    font,
                    "EDIT",
                    before,
                    WS_EX_CLIENTEDGE,
                    WS_TABSTOP | ES_AUTOHSCROLL as u32,
                    84,
                    16,
                    318,
                    22,
                    BEFORE,
                );
                label(hwnd, font, "新目录:", 16, 57, 68, -1);
                ctrl(
                    hwnd,
                    font,
                    "EDIT",
                    after,
                    WS_EX_CLIENTEDGE,
                    WS_TABSTOP | ES_AUTOHSCROLL as u32,
                    84,
                    54,
                    318,
                    22,
                    AFTER,
                );
                label(
                    hwnd,
                    font,
                    "替换选中项的链接、起始目录和图标路径中的目录前缀。",
                    16,
                    91,
                    394,
                    -1,
                );
                button(hwnd, font, "确定", 242, 130, 72, 23, IDOK);
                button(hwnd, font, "取消", 330, 130, 72, 23, IDCANCEL);
            }
            Dialog::Minimize(value) => {
                radio(
                    hwnd,
                    font,
                    "运行后最小化快速启动",
                    24,
                    20,
                    254,
                    MIN_YES,
                    true,
                    *value,
                );
                radio(
                    hwnd,
                    font,
                    "运行后不最小化快速启动",
                    24,
                    50,
                    254,
                    MIN_NO,
                    false,
                    !*value,
                );
                button(hwnd, font, "确定", 124, 94, 72, 23, IDOK);
                button(hwnd, font, "取消", 212, 94, 72, 23, IDCANCEL);
            }
            Dialog::Batch(_) => populate_manager(hwnd, font),
        }
    }
    let mut bounds = RECT {
        right: state.size.0,
        bottom: state.size.1,
        ..mem::zeroed()
    };
    AdjustWindowRectEx(
        &mut bounds,
        GetWindowLongW(hwnd, GWL_STYLE) as u32,
        0,
        GetWindowLongW(hwnd, GWL_EXSTYLE) as u32,
    );
    let mut owner: RECT = mem::zeroed();
    GetWindowRect(GetParent(hwnd), &mut owner);
    let (w, h) = (bounds.right - bounds.left, bounds.bottom - bounds.top);
    let monitor = MonitorFromWindow(GetParent(hwnd), MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = mem::zeroed();
    info.cbSize = mem::size_of::<MONITORINFO>() as u32;
    let mut x = owner.left + (owner.right - owner.left - w) / 2;
    let mut y = owner.top + (owner.bottom - owner.top - h) / 2;
    if GetMonitorInfoW(monitor, &mut info) != 0 {
        x = x.clamp(
            info.rcWork.left,
            (info.rcWork.right - w).max(info.rcWork.left),
        );
        y = y.clamp(
            info.rcWork.top,
            (info.rcWork.bottom - h).max(info.rcWork.top),
        );
    }
    SetWindowPos(hwnd, 0, x, y, w, h, SWP_NOZORDER);
    if GetDlgItem(hwnd, TAB) != 0 {
        rebuild(hwnd);
        SetTimer(hwnd, 1, 80, None);
    }
    let edit = GetDlgItem(hwnd, TITLE);
    if edit != 0 {
        SendMessageW(edit, EM_SETSEL, 0, -1);
        SetFocus(edit);
    }
}

unsafe fn populate_manager(hwnd: HWND, font: HFONT) {
    let tab = ctrl(
        hwnd,
        font,
        "SysTabControl32",
        "",
        0,
        WS_TABSTOP,
        18,
        16,
        552,
        400,
        TAB,
    );
    for (index, title) in ["批量操作", "导出快捷方式", "导入快捷方式"]
        .into_iter()
        .enumerate()
    {
        let mut title = to_wide(title);
        let mut item: TCITEMW = mem::zeroed();
        item.mask = TCIF_TEXT;
        item.pszText = title.as_mut_ptr();
        SendMessageW(tab, TCM_INSERTITEMW, index, &item as *const _ as LPARAM);
    }
    for (id, title, x, width) in [
        (DELETE, "删除", 42, 64),
        (PATH, "修改路径", 114, 64),
        (ICON, "修改图标", 186, 64),
        (MINIMIZE, "修改最小化", 258, 72),
        (ALL, "全选", 394, 48),
        (NONE, "全不选", 450, 48),
        (INVERT, "反选", 506, 48),
        (REMOVE, "删除选中", 394, 64),
    ] {
        button(hwnd, font, title, x, 58, width, 20, id);
    }
    ctrl(
        hwnd,
        font,
        "SysListView32",
        "",
        WS_EX_CLIENTEDGE,
        WS_TABSTOP | LVS_REPORT | LVS_SHOWSELALWAYS,
        42,
        84,
        512,
        316,
        LIST,
    );
    SendMessageW(
        GetDlgItem(hwnd, LIST),
        LVM_SETEXTENDEDLISTVIEWSTYLE,
        0,
        (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as LPARAM,
    );
    ctrl(
        hwnd,
        font,
        "STATIC",
        "",
        0,
        0x000D,
        42,
        56,
        18,
        18,
        SEARCH_ICON,
    );
    label(hwnd, font, "搜索快捷方式", 66, 60, 104, 1840);
    ctrl(hwnd, font, "STATIC", "", 0, 0x0010, 42, 84, 128, 2, 1841);
    label(hwnd, font, "搜索范围:", 42, 100, 128, 1842);
    let range = ctrl(
        hwnd,
        font,
        "COMBOBOX",
        "",
        0,
        WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWN as u32 | CBS_AUTOHSCROLL as u32,
        42,
        116,
        128,
        180,
        RANGE,
    );
    let drives = GetLogicalDrives();
    for index in 0..26 {
        if drives & (1 << index) != 0 {
            let drive = to_wide(&format!("{}:\\", char::from(b'A' + index)));
            SendMessageW(range, CB_ADDSTRING, 0, drive.as_ptr() as LPARAM);
        }
    }
    SendMessageW(
        range,
        CB_ADDSTRING,
        0,
        to_wide("选择文件夹...").as_ptr() as LPARAM,
    );
    SetWindowTextW(range, to_wide("C:\\").as_ptr());
    label(hwnd, font, "搜索类型:", 42, 148, 128, 1843);
    let pattern = ctrl(
        hwnd,
        font,
        "COMBOBOX",
        "",
        0,
        WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWN as u32 | CBS_AUTOHSCROLL as u32,
        42,
        164,
        128,
        160,
        PATTERN,
    );
    for text in ["*.lnk;*.url", "*.kstart.json", "*.exe", "*.*"] {
        SendMessageW(pattern, CB_ADDSTRING, 0, to_wide(text).as_ptr() as LPARAM);
    }
    SendMessageW(pattern, CB_SETCURSEL, 0, 0);
    let check = ctrl(
        hwnd,
        font,
        "BUTTON",
        "搜索子目录",
        0,
        WS_TABSTOP | BS_AUTOCHECKBOX as u32,
        42,
        196,
        128,
        18,
        RECURSIVE,
    );
    SendMessageW(check, BM_SETCHECK, 1, 0);
    button(hwnd, font, "开始(&S)", 66, 228, 80, 28, START);
    label(hwnd, font, "", 42, 434, 364, STATUS);
    button(hwnd, font, "导出", 418, 432, 72, 21, IDOK);
    button(hwnd, font, "取消", 498, 432, 72, 21, IDCANCEL);
}

unsafe fn add_column(list: HWND, index: usize, title: &str, width: i32) {
    let mut title = to_wide(title);
    let mut column: LVCOLUMNW = mem::zeroed();
    column.mask = LVCF_TEXT | LVCF_WIDTH;
    column.cx = width;
    column.pszText = title.as_mut_ptr();
    SendMessageW(
        list,
        LVM_INSERTCOLUMNW,
        index,
        &column as *const _ as LPARAM,
    );
}
unsafe fn row(list: HWND, index: usize, values: &[&str]) {
    for (column, value) in values.iter().enumerate() {
        let mut text = to_wide(value);
        let mut item: LVITEMW = mem::zeroed();
        item.mask = LVIF_TEXT;
        item.iItem = index as i32;
        item.iSubItem = column as i32;
        item.pszText = text.as_mut_ptr();
        SendMessageW(
            list,
            if column == 0 {
                LVM_INSERTITEMW
            } else {
                LVM_SETITEMTEXTW
            },
            index,
            &item as *const _ as LPARAM,
        );
    }
}
unsafe fn selection(list: HWND) -> Vec<usize> {
    let count = SendMessageW(list, LVM_GETITEMCOUNT, 0, 0).max(0) as usize;
    (0..count)
        .filter(|&index| SendMessageW(list, LVM_GETITEMSTATE, index, LVIS_SELECTED as LPARAM) != 0)
        .collect()
}
unsafe fn select(list: HWND, index: WPARAM, selected: bool) {
    let mut item: LVITEMW = mem::zeroed();
    item.stateMask = LVIS_SELECTED;
    item.state = if selected { LVIS_SELECTED } else { 0 };
    SendMessageW(list, LVM_SETITEMSTATE, index, &item as *const _ as LPARAM);
}

unsafe fn rebuild(hwnd: HWND) {
    let Some(state) = state(hwnd) else {
        return;
    };
    let snapshot = state.dialog.try_borrow().ok().and_then(|dialog| {
        if let Dialog::Batch(m) = &*dialog {
            Some((
                m.tab,
                m.items.clone(),
                m.paths.clone(),
                m.selection[m.tab].clone(),
                m.receiver.is_some(),
            ))
        } else {
            None
        }
    });
    let Some((tab, items, paths, selected, scanning)) = snapshot else {
        return;
    };
    for id in [DELETE, PATH, ICON, MINIMIZE] {
        ShowWindow(
            GetDlgItem(hwnd, id),
            if tab == 0 { SW_SHOW } else { SW_HIDE },
        );
    }
    for id in [
        SEARCH_ICON,
        1840,
        1841,
        1842,
        1843,
        RANGE,
        PATTERN,
        RECURSIVE,
        START,
        REMOVE,
        STATUS,
    ] {
        ShowWindow(
            GetDlgItem(hwnd, id),
            if tab == 2 { SW_SHOW } else { SW_HIDE },
        );
    }
    for (offset, id) in [ALL, NONE, INVERT].into_iter().enumerate() {
        SetWindowPos(
            GetDlgItem(hwnd, id),
            0,
            if tab == 2 {
                178 + offset as i32 * 72
            } else {
                394 + offset as i32 * 56
            },
            58,
            if tab == 2 { 64 } else { 48 },
            20,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    let list = GetDlgItem(hwnd, LIST);
    SendMessageW(list, WM_SETREDRAW, 0, 0);
    SendMessageW(list, LVM_DELETEALLITEMS, 0, 0);
    while SendMessageW(list, LVM_DELETECOLUMN, 0, 0) != 0 {}
    SetWindowPos(
        list,
        0,
        if tab == 2 { 178 } else { 42 },
        84,
        if tab == 2 { 374 } else { 512 },
        316,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    if tab == 2 {
        for (i, (title, width)) in [("文件名", 134), ("所在路径", 214), ("", 22)]
            .into_iter()
            .enumerate()
        {
            add_column(list, i, title, width);
        }
        for (i, path) in paths.iter().enumerate() {
            row(
                list,
                i,
                &[
                    &path.file_name().unwrap_or_default().to_string_lossy(),
                    &path.parent().unwrap_or(path).to_string_lossy(),
                ],
            );
        }
    } else {
        for (i, (title, width)) in [
            ("标题", 124),
            ("链接", 174),
            ("图标", 138),
            ("运行后", 56),
            ("", 16),
        ]
        .into_iter()
        .enumerate()
        {
            add_column(list, i, title, width);
        }
        for (i, item) in items.iter().enumerate() {
            row(
                list,
                i,
                &[
                    &item.name,
                    &item.target_path,
                    &item.icon_key,
                    if item.minimize_on_run {
                        "最小化"
                    } else {
                        ""
                    },
                ],
            );
        }
    }
    for index in selected {
        select(list, index, true);
    }
    SendMessageW(list, WM_SETREDRAW, 1, 0);
    InvalidateRect(list, ptr::null(), 1);
    ShowWindow(
        GetDlgItem(hwnd, IDOK),
        if tab == 0 { SW_HIDE } else { SW_SHOW },
    );
    set_ctrl_text(hwnd, IDOK, if tab == 2 { "导入" } else { "导出" });
    EnableWindow(GetDlgItem(hwnd, IDOK), (!scanning) as i32);
    InvalidateRect(hwnd, ptr::null(), 1);
}

unsafe fn notify(hwnd: HWND, message: &str) {
    #[cfg(test)]
    if crate::dialog_tests::active() {
        return;
    }
    MessageBoxW(
        hwnd,
        to_wide(message).as_ptr(),
        to_wide("KStart").as_ptr(),
        MB_OK | MB_ICONINFORMATION,
    );
}

unsafe fn batch_command(hwnd: HWND, id: i32, notification: u32) {
    let Some(state) = state(hwnd) else {
        return;
    };
    let snapshot = state.dialog.try_borrow().ok().and_then(|dialog| {
        if let Dialog::Batch(m) = &*dialog {
            Some((
                m.tab,
                m.column.clone(),
                m.items.clone(),
                m.paths.clone(),
                m.base.clone(),
                m.settings.clone(),
                m.receiver.is_some(),
            ))
        } else {
            None
        }
    });
    let Some((tab, column, items, paths, base, settings, scanning)) = snapshot else {
        return;
    };
    let list = GetDlgItem(hwnd, LIST);
    let selected = selection(list);
    if [ALL, NONE, INVERT].contains(&id) {
        let count = SendMessageW(list, LVM_GETITEMCOUNT, 0, 0).max(0) as usize;
        for i in 0..count {
            select(list, i, id == ALL || id == INVERT && !selected.contains(&i));
        }
        return;
    }
    if id == RANGE && notification == CBN_SELCHANGE && get_ctrl_text(hwnd, RANGE) == "选择文件夹..."
    {
        if let Some(path) = crate::pick_folder_dialog(hwnd, "搜索快捷方式") {
            set_ctrl_text(hwnd, RANGE, &path.to_string_lossy());
        } else {
            set_ctrl_text(hwnd, RANGE, "C:\\");
        }
        return;
    }
    if id == START {
        if scanning {
            if let Ok(dialog) = state.dialog.try_borrow() {
                if let Dialog::Batch(m) = &*dialog {
                    m.cancel.store(true, Ordering::Relaxed);
                }
            }
            return;
        }
        let root = PathBuf::from(get_ctrl_text(hwnd, RANGE).trim());
        let patterns = get_ctrl_text(hwnd, PATTERN);
        if !root.is_dir() {
            notify(hwnd, "请选择有效的搜索目录。");
            return;
        }
        if patterns.trim().is_empty() {
            notify(hwnd, "请输入搜索类型，例如 *.lnk;*.url。");
            return;
        }
        let recursive = SendMessageW(GetDlgItem(hwnd, RECURSIVE), BM_GETCHECK, 0, 0) != 0;
        if let Ok(mut dialog) = state.dialog.try_borrow_mut() {
            if let Dialog::Batch(m) = &mut *dialog {
                m.paths.clear();
                m.selection[2].clear();
                m.cancel = Arc::new(AtomicBool::new(false));
                m.receiver = Some(shortcut_transfer::scan(
                    root,
                    patterns,
                    recursive,
                    m.cancel.clone(),
                ));
            }
        }
        set_ctrl_text(hwnd, START, "停止(&S)");
        set_ctrl_text(hwnd, STATUS, "正在搜索...");
        for control in [RANGE, PATTERN, RECURSIVE, REMOVE] {
            EnableWindow(GetDlgItem(hwnd, control), 0);
        }
        rebuild(hwnd);
        return;
    }
    if id == REMOVE && tab == 2 {
        if let Ok(mut dialog) = state.dialog.try_borrow_mut() {
            if let Dialog::Batch(m) = &mut *dialog {
                for &i in selected.iter().rev() {
                    if i < m.paths.len() {
                        m.paths.remove(i);
                    }
                }
                m.selection[2].clear();
            }
        }
        rebuild(hwnd);
        return;
    }
    if ![DELETE, PATH, ICON, MINIMIZE, IDOK].contains(&id) {
        return;
    }
    if selected.is_empty() {
        notify(hwnd, "请先选择要操作的快捷方式。");
        return;
    }
    if tab == 2 && id == IDOK {
        if scanning {
            return;
        }
        let mut imported = Vec::new();
        let mut failures = Vec::new();
        let mut completed = Vec::new();
        for index in selected {
            if let Some(path) = paths.get(index) {
                match shortcut_transfer::import_file(path, &base, &settings) {
                    Ok(mut items) => {
                        imported.append(&mut items);
                        completed.push(index);
                    }
                    Err(error) => failures.push(format!("{}：{error}", path.display())),
                }
            }
        }
        let count = imported.len();
        if count > 0 {
            let first = APP_STATE.with(|cell| {
                cell.try_borrow().ok().and_then(|guard| {
                    guard
                        .as_ref()
                        .map(|app| app.data_store.next_shortcut_number())
                })
            });
            let Some(first) = first else {
                return;
            };
            for (offset, item) in imported.iter_mut().enumerate() {
                item.id = format!("sc_{}", first as u64 + offset as u64);
            }
            let updated = mutate(&column, |group| {
                for mut item in imported {
                    item.enter_column(&group.category_id, &group.id);
                    group.items.push(item);
                }
            });
            if updated.is_none() {
                notify(hwnd, "目标栏目已不存在。");
                return;
            }
            if let Ok(mut dialog) = state.dialog.try_borrow_mut() {
                if let Dialog::Batch(m) = &mut *dialog {
                    m.items = updated.unwrap();
                    for index in completed.into_iter().rev() {
                        m.paths.remove(index);
                    }
                    m.selection[2].clear();
                }
            }
        }
        rebuild(hwnd);
        notify(
            hwnd,
            &format!("已导入 {count} 个快捷方式。{}", failure_summary(&failures)),
        );
        return;
    }
    let chosen: Vec<_> = selected
        .iter()
        .filter_map(|&i| items.get(i).cloned())
        .collect();
    let ids: HashSet<_> = chosen.iter().map(|item| item.id.clone()).collect();
    if tab == 1 && id == IDOK {
        if let Some(folder) = crate::pick_folder_dialog(hwnd, "选择快捷方式导出目录") {
            match shortcut_transfer::export(&base, &folder, &chosen) {
                Ok(report) => notify(
                    hwnd,
                    &format!(
                        "已导出 {} 个快捷方式。\n完整配置：{}\nWindows 快捷方式：{} 个。{}",
                        chosen.len(),
                        report.archive.display(),
                        report.native_count,
                        failure_summary(&report.failures)
                    ),
                ),
                Err(error) => notify(hwnd, &format!("导出失败：{error}")),
            }
        }
        return;
    }
    let updated = match id {
        DELETE => {
            if !crate::advanced_runtime::confirm(
                hwnd,
                &format!("确定删除选中的 {} 个快捷方式吗？", chosen.len()),
            ) {
                return;
            }
            mutate(&column, |group| {
                group.items.retain(|item| !ids.contains(&item.id))
            })
        }
        PATH => {
            let before = chosen
                .first()
                .and_then(|item| std::path::Path::new(&item.target_path).parent())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let Some(Dialog::Paths { before, after }) = show(
                hwnd,
                "批量修改路径",
                (420, 168),
                Dialog::Paths {
                    before,
                    after: String::new(),
                },
            ) else {
                return;
            };
            mutate(&column, |group| {
                for item in &mut group.items {
                    if ids.contains(&item.id) {
                        if matches!(
                            item.target_kind,
                            crate::targets::TargetKind::File | crate::targets::TargetKind::Folder
                        ) {
                            item.target_path = shortcut_transfer::replace_path_prefix(
                                &item.target_path,
                                &before,
                                &after,
                            );
                        }
                        item.work_dir =
                            shortcut_transfer::replace_path_prefix(&item.work_dir, &before, &after);
                        item.icon_key =
                            shortcut_transfer::replace_path_prefix(&item.icon_key, &before, &after);
                    }
                }
            })
        }
        ICON => {
            let Some(path) = crate::pick_file_dialog(
                hwnd,
                "选择程序或图片作为图标",
                &[
                    ("程序与图标图片", "*.exe;*.dll;*.ico;*.png;*.bmp;*.jpg"),
                    ("所有文件", "*.*"),
                ],
            ) else {
                return;
            };
            let Some(icon) = crate::skin_assets::load_custom_icon_16(&path) else {
                notify(hwnd, "无法读取所选图标。");
                return;
            };
            mutate(&column, |group| {
                for item in &mut group.items {
                    if ids.contains(&item.id) {
                        item.icon_key = path.to_string_lossy().into_owned();
                        item.custom_icon_16 = Some(icon.clone());
                    }
                }
            })
        }
        MINIMIZE => {
            let value = chosen.first().is_some_and(|item| item.minimize_on_run);
            let Some(Dialog::Minimize(value)) =
                show(hwnd, "修改最小化", (300, 130), Dialog::Minimize(value))
            else {
                return;
            };
            mutate(&column, |group| {
                for item in &mut group.items {
                    if ids.contains(&item.id) {
                        item.minimize_on_run = value;
                    }
                }
            })
        }
        _ => return,
    };
    if let Some(items) = updated {
        if let Ok(mut dialog) = state.dialog.try_borrow_mut() {
            if let Dialog::Batch(m) = &mut *dialog {
                m.items = items;
                m.selection[0] = m
                    .items
                    .iter()
                    .enumerate()
                    .filter_map(|(index, item)| ids.contains(&item.id).then_some(index))
                    .collect();
                m.selection[1] = m.selection[0].clone();
            }
        }
        rebuild(hwnd);
    }
}

fn failure_summary(failures: &[String]) -> String {
    if failures.is_empty() {
        String::new()
    } else {
        format!(
            "\n失败 {} 项：\n{}",
            failures.len(),
            failures
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

unsafe fn poll(hwnd: HWND) {
    let Some(state) = state(hwnd) else {
        return;
    };
    let mut added = Vec::new();
    let mut finished = None;
    let mut import_tab = false;
    let mut count = 0;
    if let Ok(mut dialog) = state.dialog.try_borrow_mut() {
        if let Dialog::Batch(m) = &mut *dialog {
            if let Some(receiver) = &m.receiver {
                for _ in 0..16 {
                    match receiver.try_recv() {
                        Ok(ScanEvent::Files(mut paths)) => added.append(&mut paths),
                        Ok(ScanEvent::Finished(skipped)) => {
                            finished = Some(skipped);
                            break;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            finished = Some(0);
                            break;
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                    }
                }
            }
            count = m.paths.len();
            m.paths.extend(added.clone());
            import_tab = m.tab == 2;
            if finished.is_some() {
                m.receiver = None;
            }
        }
    }
    if import_tab && !added.is_empty() {
        let list = GetDlgItem(hwnd, LIST);
        SendMessageW(list, WM_SETREDRAW, 0, 0);
        for (index, path) in added.iter().enumerate() {
            row(
                list,
                count + index,
                &[
                    &path.file_name().unwrap_or_default().to_string_lossy(),
                    &path.parent().unwrap_or(path).to_string_lossy(),
                ],
            );
        }
        SendMessageW(list, WM_SETREDRAW, 1, 0);
        InvalidateRect(list, ptr::null(), 0);
    }
    if !added.is_empty() || finished.is_some() {
        set_ctrl_text(
            hwnd,
            STATUS,
            &format!(
                "已找到 {} 项{}",
                count + added.len(),
                finished
                    .filter(|&n| n > 0)
                    .map(|n| format!("，跳过 {n} 个无法读取的目录"))
                    .unwrap_or_default()
            ),
        );
    }
    if finished.is_some() {
        set_ctrl_text(hwnd, START, "开始(&S)");
        for id in [RANGE, PATTERN, RECURSIVE, REMOVE, IDOK] {
            EnableWindow(GetDlgItem(hwnd, id), 1);
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> isize {
    if msg == WM_INITDIALOG {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, lp);
        if let Some(state) = state(hwnd) {
            populate(hwnd, state);
        }
        #[cfg(test)]
        crate::dialog_tests::arm(hwnd);
        return if GetDlgItem(hwnd, TITLE) != 0 { 0 } else { 1 };
    }
    if msg == WM_CLOSE || msg == WM_COMMAND && wp as u16 as i32 == IDCANCEL {
        EndDialog(hwnd, IDCANCEL as isize);
        return 1;
    }
    if msg == WM_ERASEBKGND {
        let mut rect: RECT = mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        FillRect(wp as HDC, &rect, GetSysColorBrush(COLOR_BTNFACE));
        return 1;
    }
    if msg == WM_CTLCOLORSTATIC {
        SetTextColor(wp as HDC, 0);
        SetBkColor(wp as HDC, GetSysColor(COLOR_BTNFACE));
        return GetSysColorBrush(COLOR_BTNFACE);
    }
    if msg == WM_DRAWITEM && !(lp as *const DRAWITEMSTRUCT).is_null() {
        let draw = &*(lp as *const DRAWITEMSTRUCT);
        if draw.CtlID == SEARCH_ICON as u32 {
            FillRect(draw.hDC, &draw.rcItem, GetSysColorBrush(COLOR_BTNFACE));
            if let Ok(image) =
                image::load_from_memory(include_bytes!("../assets/icons/ctx_search.png"))
            {
                let image = image.to_rgba8();
                let icon = crate::skin_assets::RgbaSurface::from_pixels(
                    image.width(),
                    image.height(),
                    image.pixels().map(|p| p.0).collect(),
                );
                let color = GetSysColor(COLOR_BTNFACE).to_le_bytes();
                crate::native_dialogs::blit_surface_on_bg(
                    draw.hDC,
                    &icon,
                    0,
                    0,
                    16,
                    16,
                    [color[0], color[1], color[2]],
                );
            }
            return 1;
        }
    }
    if msg == WM_TIMER {
        #[cfg(test)]
        if crate::dialog_tests::step(hwnd, wp) {
            return 1;
        }
        poll(hwnd);
        return 1;
    }
    let Some(state) = state(hwnd) else {
        return 0;
    };
    if msg == WM_NOTIFY && lp != 0 {
        let header = &*(lp as *const NMHDR);
        if header.idFrom == TAB as usize && header.code == TCN_SELCHANGING {
            let selected = selection(GetDlgItem(hwnd, LIST));
            if let Ok(mut dialog) = state.dialog.try_borrow_mut() {
                if let Dialog::Batch(m) = &mut *dialog {
                    m.selection[m.tab] = selected;
                }
            }
        }
        if header.idFrom == TAB as usize && header.code == TCN_SELCHANGE {
            let tab = SendMessageW(GetDlgItem(hwnd, TAB), TCM_GETCURSEL, 0, 0).clamp(0, 2) as usize;
            if let Ok(mut dialog) = state.dialog.try_borrow_mut() {
                if let Dialog::Batch(m) = &mut *dialog {
                    m.tab = tab;
                }
            }
            rebuild(hwnd);
            return 1;
        }
    }
    if msg == WM_COMMAND {
        let id = wp as u16 as i32;
        let batch = state
            .dialog
            .try_borrow()
            .ok()
            .is_some_and(|d| matches!(*d, Dialog::Batch(_)));
        if batch {
            batch_command(hwnd, id, (wp >> 16) as u32);
            return 1;
        }
        if id == IDOK {
            let value = get_ctrl_text(hwnd, TITLE);
            let before = get_ctrl_text(hwnd, BEFORE).trim().to_string();
            let after = get_ctrl_text(hwnd, AFTER).trim().to_string();
            let mut error = None;
            let Ok(mut dialog) = state.dialog.try_borrow_mut() else {
                return 1;
            };
            {
                match &mut *dialog {
                    Dialog::Sort { key, descending } => {
                        *key = if SendMessageW(GetDlgItem(hwnd, SORT_TIME), BM_GETCHECK, 0, 0) != 0
                        {
                            ShortcutSortKey::ColumnAddedAt
                        } else if SendMessageW(GetDlgItem(hwnd, SORT_PATH), BM_GETCHECK, 0, 0) != 0
                        {
                            ShortcutSortKey::Path
                        } else {
                            ShortcutSortKey::Title
                        };
                        *descending =
                            SendMessageW(GetDlgItem(hwnd, DESCENDING), BM_GETCHECK, 0, 0) != 0;
                    }
                    Dialog::Rename(title) | Dialog::Create(title) => {
                        if value.trim().is_empty() {
                            error = Some("栏目标题不能为空。");
                        } else {
                            *title = value;
                        }
                    }
                    Dialog::Paths {
                        before: old,
                        after: new,
                    } => {
                        if before.is_empty() || after.is_empty() {
                            error = Some("请填写原目录和新目录。");
                        } else {
                            *old = before;
                            *new = after;
                        }
                    }
                    Dialog::Minimize(value) => {
                        *value = SendMessageW(GetDlgItem(hwnd, MIN_YES), BM_GETCHECK, 0, 0) != 0
                    }
                    _ => {}
                }
            }
            drop(dialog);
            if let Some(error) = error {
                notify(hwnd, error);
            } else {
                EndDialog(hwnd, IDOK as isize);
            }
            return 1;
        }
    }
    0
}

#[cfg(test)]
mod create_tests {
    use super::*;
    use crate::{
        dialog_tests::{capture, enable, queue},
        renderer::{ActivePopupMenu, HitTarget},
        skin_parser::ToolButtonKind,
        AppState,
    };
    unsafe fn click_add(owner: HWND, x: i32, y: i32) {
        let position = (x & 0xffff) as isize | (((y & 0xffff) as isize) << 16);
        crate::main_wnd_proc(owner, WM_LBUTTONDOWN, 1, position);
        crate::main_wnd_proc(owner, WM_LBUTTONUP, 0, position);
    }
    #[test]
    fn create_column_button_commits_only_after_confirmation_when_requested() {
        let Some(base) = std::env::var_os("KSTART_CREATE_COLUMN_UI_BASE") else {
            return;
        };
        let base = PathBuf::from(base);
        let shots = base.join("shots");
        std::fs::create_dir_all(&shots).unwrap();
        let mut app = AppState::new(base.clone(), ActivePopupMenu::None).unwrap();
        app.persist_data();
        app.selected_shortcuts.insert(0);
        app.advanced_window.shortcut_scroll = 2;
        let before = std::fs::read(base.join("user/data.json")).unwrap();
        let count = app.categories[0].groups.len();
        let other = app.categories[1].groups.len();
        let category = app.categories[0].id.clone();
        let button = app
            .skin_cfg
            .tool_buttons
            .iter()
            .find(|button| button.kind == ToolButtonKind::FolderLock)
            .unwrap();
        let x = button
            .left
            .resolve(app.user_cfg.window_width, app.user_cfg.window_height)
            + 2;
        let y = button
            .top
            .resolve(app.user_cfg.window_width, app.user_cfg.window_height)
            + 2;
        assert_eq!(
            crate::renderer::hit_test_main(
                x,
                y,
                app.user_cfg.window_width,
                app.user_cfg.window_height,
                &app.skin_cfg,
                &app.skin_assets,
                &app.categories,
                0,
                &app.user_cfg.advanced,
                0
            ),
            HitTarget::ToolButton(ToolButtonKind::FolderLock)
        );
        unsafe {
            let owner = CreateWindowExW(
                0,
                to_wide("STATIC").as_ptr(),
                to_wide("KStart 栏目测试").as_ptr(),
                WS_POPUP,
                0,
                0,
                app.user_cfg.window_width,
                app.user_cfg.window_height,
                0,
                0,
                GetModuleHandleW(ptr::null()),
                ptr::null(),
            );
            assert_ne!(owner, 0);
            app.main_hwnd = owner;
            APP_STATE.with(|cell| *cell.try_borrow_mut().unwrap() = Some(app));
            enable(true);
            let shot = shots.join("column-create.png");
            queue(move |hwnd| {
                assert_eq!(get_ctrl_text(hwnd, TITLE), "新建栏目");
                assert_eq!(
                    SendMessageW(GetDlgItem(hwnd, NORMAL_COLUMN), BM_GETCHECK, 0, 0),
                    1
                );
                assert_eq!(
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(GetDlgItem(
                        hwnd,
                        FOLDER_COLUMN
                    )),
                    0
                );
                let selection = SendMessageW(GetDlgItem(hwnd, TITLE), EM_GETSEL, 0, 0) as u32;
                assert_eq!(selection & 0xffff, 0);
                assert_eq!(selection >> 16, 4);
                capture(hwnd, &shot);
                set_ctrl_text(hwnd, TITLE, "取消后不得新增");
                SendMessageW(hwnd, WM_COMMAND, IDCANCEL as usize, 0);
            });
            click_add(owner, x, y);
            assert_eq!(std::fs::read(base.join("user/data.json")).unwrap(), before);
            queue(move |hwnd| {
                set_ctrl_text(hwnd, TITLE, "   ");
                SendMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
                assert_ne!(IsWindow(hwnd), 0);
                APP_STATE.with(|cell| {
                    assert_eq!(
                        cell.try_borrow().unwrap().as_ref().unwrap().categories[0]
                            .groups
                            .len(),
                        count
                    )
                });
                set_ctrl_text(hwnd, TITLE, "  工作资料  ");
                SendMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
            });
            click_add(owner, x, y);
            APP_STATE.with(|cell| {
                let guard = cell.try_borrow().unwrap();
                let app = guard.as_ref().unwrap();
                assert_eq!(app.categories[0].groups.len(), count + 1);
                assert_eq!(app.categories[0].active_group_index, count);
                assert_eq!(app.categories[0].groups[count].title, "  工作资料  ");
                assert!(app.categories[0].groups[count].items.is_empty());
                assert!(app.selected_shortcuts.is_empty());
                assert_eq!(app.advanced_window.shortcut_scroll, 0);
                assert_eq!(app.categories[1].groups.len(), other);
            });
            let saved = crate::model::AppDataStore::load(&base);
            assert!(saved
                .columns
                .iter()
                .any(|column| column.category_id == category && column.name == "  工作资料  "));
            APP_STATE.with(|cell| {
                let mut guard = cell.try_borrow_mut().unwrap();
                let app = guard.as_mut().unwrap();
                app.categories[0].groups[count].title = "-= 新建项目 =-".into();
                app.persist_data();
            });
            let shot = shots.join("column-rename-literal.png");
            queue(move |hwnd| {
                assert_eq!(get_ctrl_text(hwnd, TITLE), "-= 新建项目 =-");
                capture(hwnd, &shot);
                set_ctrl_text(hwnd, TITLE, "  -= 修改后的项目 =-  ");
                SendMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
            });
            open(owner, 0, count, Action::Rename);
            let saved = crate::model::AppDataStore::load(&base);
            assert!(saved
                .columns
                .iter()
                .any(|column| column.category_id == category
                    && column.name == "  -= 修改后的项目 =-  "));
            let tree = saved.build_runtime_tree(&base, &Default::default(), &Default::default());
            assert_eq!(tree[0].groups[count].title, "  -= 修改后的项目 =-  ");
            APP_STATE.with(|cell| {
                let mut guard = cell.try_borrow_mut().unwrap();
                let app = guard.as_mut().unwrap();
                app.categories[0].groups.clear();
                app.categories[0].active_group_index = 0;
                app.persist_data();
            });
            queue(|hwnd| {
                SendMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
            });
            click_add(owner, x, y);
            APP_STATE.with(|cell| {
                let guard = cell.try_borrow().unwrap();
                let app = guard.as_ref().unwrap();
                assert_eq!(app.categories[0].groups.len(), 1);
                assert_eq!(app.categories[0].groups[0].title, "新建栏目");
                let ids = app
                    .categories
                    .iter()
                    .flat_map(|cat| &cat.groups)
                    .map(|group| &group.id)
                    .collect::<HashSet<_>>();
                assert_eq!(
                    ids.len(),
                    app.categories
                        .iter()
                        .map(|cat| cat.groups.len())
                        .sum::<usize>()
                );
            });
            let before = std::fs::read(base.join("user/data.json")).unwrap();
            queue(|hwnd| {
                SendMessageW(hwnd, WM_CLOSE, 0, 0);
            });
            click_add(owner, x, y);
            assert_eq!(std::fs::read(base.join("user/data.json")).unwrap(), before);
            enable(false);
            DestroyWindow(owner);
        }
        assert!(!base.join("user/icons").exists());
        assert!(!base.join("user/icon-cache").exists());
        APP_STATE.with(|cell| {
            cell.try_borrow_mut().unwrap().take();
        });
    }
}
