//! 计划任务管理、编辑、提醒和批处理；窗口布局按参考图使用经典原生控件。
use crate::{
    native_dialogs::{create_ctrl, get_ctrl_text, set_ctrl_text},
    schedule::{self, Command, ReminderOptions, Step, Task, Trigger},
    to_wide, APP_STATE,
};
use chrono::{Datelike, NaiveDate, NaiveTime, Timelike};
use std::{cell::RefCell, mem, path::PathBuf, ptr};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::*,
        Input::KeyboardAndMouse::{EnableWindow, SetFocus},
        WindowsAndMessaging::*,
    },
};
const SS_ETCHEDFRAME: u32 = 0x12;
const SS_CENTER: u32 = 1;
const MANAGER_TIMER: usize = 32;
const LIST: i32 = 1701;
const ADD: i32 = 1702;
const DELETE: i32 = 1703;
const MODIFY: i32 = 1704;
const NAME: i32 = 1710;
const LINK: i32 = 1711;
const ARGS: i32 = 1712;
const START: i32 = 1713;
const BATCH: i32 = 1714;
const REMIND: i32 = 1715;
const TEST: i32 = 1716;
const CONVERT: i32 = 1717;
const FILE: i32 = 1720;
const FOLDER: i32 = 1721;
const SYSTEM: i32 = 1722;
const SHORTCUT: i32 = 1723;
const TIME: i32 = 1730;
const ENABLED: i32 = 1731;
const MONTH: i32 = 1732;
const YEAR: i32 = 1733;
const TIME_LABEL: i32 = 1734;
const UP: i32 = 1740;
const DOWN: i32 = 1741;
const DELAY: i32 = 1742;
const ADD_DELAY: i32 = 1743;
const ADD_STEP: i32 = 1744;
const BODY: i32 = 1745;
const AUTO_CLOSE: i32 = 1750;
const SOUND_ENABLED: i32 = 1751;
const SOUND_FILE: i32 = 1752;
const SOUND_PICK: i32 = 1753;
const SOUND_PREVIEW: i32 = 1754;
const RADIO: i32 = 1800;
const DAY: i32 = 1900;
#[derive(Clone)]
enum Dialog {
    Manager {
        ids: Vec<String>,
    },
    Editor {
        task: Task,
        batch: bool,
    },
    Batch {
        steps: Vec<Step>,
        editing: Option<usize>,
    },
    Reminder {
        text: String,
        options: ReminderOptions,
    },
}
struct State {
    dialog: RefCell<Dialog>,
    base: PathBuf,
    main: HWND,
    font: HFONT,
    size: (i32, i32),
    preview: RefCell<Option<crate::schedule_reminder::Playback>>,
}
fn snapshot() -> Option<(HWND, PathBuf, String)> {
    APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        (!app.user_cfg.startup_lock).then(|| {
            (
                app.main_hwnd,
                app.base_dir.clone(),
                app.scheduler.session.clone(),
            )
        })
    })
}
fn tasks() -> Vec<Task> {
    APP_STATE
        .with(|cell| {
            cell.try_borrow()
                .ok()
                .and_then(|guard| guard.as_ref().map(|app| app.plan.tasks.clone()))
        })
        .unwrap_or_default()
}
fn commit(base: &std::path::Path, task: Option<Task>, delete: Option<&str>) -> Result<(), String> {
    APP_STATE.with(|cell| {
        let mut guard = cell
            .try_borrow_mut()
            .map_err(|_| "状态正忙，请稍后重试。")?;
        let app = guard.as_mut().ok_or("应用未初始化。")?;
        let mut plan = app.plan.clone();
        let changed_id = task
            .as_ref()
            .map(|task| task.id.clone())
            .or_else(|| delete.map(str::to_owned));
        if let Some(id) = delete {
            plan.tasks.retain(|task| task.id != id);
        }
        if let Some(task) = task {
            if let Some(old) = plan.tasks.iter_mut().find(|old| old.id == task.id) {
                *old = task;
            } else {
                plan.tasks.push(task);
            }
        }
        schedule::save(base, &plan).map_err(|e| format!("保存 user/plan.json 失败：{e}"))?;
        app.plan = plan;
        if let Some(id) = changed_id {
            app.scheduler.cancel(&id);
        }
        Ok(())
    })
}
pub fn open(shortcut: Option<&str>) {
    let Some((main, base, session)) = snapshot() else {
        return;
    };
    unsafe {
        if let Some(id) = shortcut {
            let command = APP_STATE.with(|cell| {
                let guard = cell.try_borrow().ok()?;
                let item = guard
                    .as_ref()?
                    .categories
                    .iter()
                    .flat_map(|cat| &cat.groups)
                    .flat_map(|group| &group.items)
                    .find(|item| item.id == id)?;
                Some((
                    item.name.clone(),
                    Command {
                        link: item.target_path.clone(),
                        args: item.args.clone(),
                        work_dir: item.work_dir.clone(),
                    },
                ))
            });
            if let Some((name, command)) = command {
                let mut task = Task::new(crate::local_clock::now(), &session);
                task.name = name;
                task.steps.push(Step::Launch { command });
                edit(main, &base, task);
            }
        } else {
            show(
                main,
                main,
                base,
                "计划任务管理",
                (540, 500),
                Dialog::Manager { ids: Vec::new() },
            );
        }
    }
}
unsafe fn show(
    owner: HWND,
    main: HWND,
    base: PathBuf,
    title: &str,
    size: (i32, i32),
    dialog: Dialog,
) -> Option<Dialog> {
    let manager = matches!(dialog, Dialog::Manager { .. });
    let style = WS_POPUP
        | WS_CAPTION
        | WS_SYSMENU
        | DS_CENTER as u32
        | if manager {
            WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX
        } else {
            0
        };
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
        0,
        0,
        DEFAULT_QUALITY as u32,
        0,
        to_wide("宋体").as_ptr(),
    );
    let state = State {
        dialog: RefCell::new(dialog),
        base,
        main,
        font: if font != 0 {
            font
        } else {
            GetStockObject(DEFAULT_GUI_FONT) as HFONT
        },
        size,
        preview: RefCell::new(None),
    };
    let result = DialogBoxIndirectParamW(
        GetModuleHandleW(ptr::null()),
        template.as_ptr().cast(),
        owner,
        Some(proc),
        &state as *const _ as LPARAM,
    );
    if font != 0 {
        DeleteObject(font);
    }
    (result == IDOK as isize).then(|| state.dialog.into_inner())
}
unsafe fn ctrl(
    hwnd: HWND,
    s: &State,
    class: &str,
    text: &str,
    id: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    style: u32,
    ex: u32,
) -> HWND {
    create_ctrl(hwnd, s.font, class, text, ex, style, x, y, w, h, id)
}
unsafe fn label(hwnd: HWND, s: &State, text: &str, x: i32, y: i32, w: i32) {
    ctrl(hwnd, s, "STATIC", text, -1, x, y, w, 18, 0, 0);
}
unsafe fn button(hwnd: HWND, s: &State, text: &str, id: i32, x: i32, y: i32, w: i32) {
    ctrl(
        hwnd,
        s,
        "BUTTON",
        text,
        id,
        x,
        y,
        w,
        22,
        WS_TABSTOP
            | if id == IDOK {
                BS_DEFPUSHBUTTON as u32
            } else {
                0
            },
        0,
    );
}
unsafe fn tool(hwnd: HWND, s: &State, text: &str, id: i32, x: i32, y: i32, w: i32) {
    ctrl(
        hwnd,
        s,
        "BUTTON",
        text,
        id,
        x,
        y,
        w,
        24,
        WS_TABSTOP | BS_OWNERDRAW as u32,
        0,
    );
}
unsafe fn field(hwnd: HWND, s: &State, text: &str, id: i32, value: &str, y: i32, width: i32) {
    label(hwnd, s, text, 8, y + 2, 40);
    ctrl(
        hwnd,
        s,
        "EDIT",
        value,
        id,
        50,
        y,
        width,
        20,
        WS_TABSTOP | ES_AUTOHSCROLL as u32,
        WS_EX_CLIENTEDGE,
    );
}
unsafe fn list(
    hwnd: HWND,
    s: &State,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    columns: &[(&str, i32)],
    grid: bool,
) {
    let list = ctrl(
        hwnd,
        s,
        "SysListView32",
        "",
        LIST,
        x,
        y,
        w,
        h,
        WS_TABSTOP | LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS,
        WS_EX_CLIENTEDGE,
    );
    SendMessageW(
        list,
        LVM_SETEXTENDEDLISTVIEWSTYLE,
        0,
        (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | if grid { LVS_EX_GRIDLINES } else { 0 })
            as LPARAM,
    );
    for (index, (title, width)) in columns.iter().enumerate() {
        let mut text = to_wide(title);
        let mut col: LVCOLUMNW = mem::zeroed();
        col.mask = LVCF_TEXT | LVCF_WIDTH;
        col.cx = *width;
        col.pszText = text.as_mut_ptr();
        SendMessageW(list, LVM_INSERTCOLUMNW, index, &col as *const _ as LPARAM);
    }
}
unsafe fn rows(hwnd: HWND, values: Vec<Vec<String>>, selected: Option<usize>) {
    let list = GetDlgItem(hwnd, LIST);
    SendMessageW(list, WM_SETREDRAW, 0, 0);
    SendMessageW(list, LVM_DELETEALLITEMS, 0, 0);
    for (row, values) in values.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            let mut text = to_wide(value);
            let mut item: LVITEMW = mem::zeroed();
            item.mask = LVIF_TEXT;
            item.iItem = row as i32;
            item.iSubItem = column as i32;
            item.pszText = text.as_mut_ptr();
            SendMessageW(
                list,
                if column == 0 {
                    LVM_INSERTITEMW
                } else {
                    LVM_SETITEMTEXTW
                },
                if column == 0 { 0 } else { row },
                &item as *const _ as LPARAM,
            );
        }
    }
    if let Some(row) = selected.filter(|row| *row < values.len()) {
        let mut item: LVITEMW = mem::zeroed();
        item.stateMask = LVIS_SELECTED | LVIS_FOCUSED;
        item.state = item.stateMask;
        SendMessageW(list, LVM_SETITEMSTATE, row, &item as *const _ as LPARAM);
        SendMessageW(list, LVM_ENSUREVISIBLE, row, 0);
    }
    SendMessageW(list, WM_SETREDRAW, 1, 0);
    InvalidateRect(list, ptr::null(), 1);
}
unsafe fn selected(hwnd: HWND) -> Option<usize> {
    let row = SendMessageW(
        GetDlgItem(hwnd, LIST),
        LVM_GETNEXTITEM,
        usize::MAX,
        LVNI_SELECTED as LPARAM,
    );
    (row >= 0).then_some(row as usize)
}
unsafe fn refresh_manager(hwnd: HWND, s: &State, selection: Option<usize>) {
    let tasks = tasks();
    let values = tasks
        .iter()
        .map(|task| {
            vec![
                task.trigger.label().into(),
                task.summary(),
                task.name.clone(),
            ]
        })
        .collect();
    let ids = tasks.into_iter().map(|task| task.id).collect();
    if let Ok(mut state) = s.dialog.try_borrow_mut() {
        *state = Dialog::Manager { ids };
    }
    rows(hwnd, values, selection);
    EnableWindow(
        GetDlgItem(hwnd, MODIFY),
        if selected(hwnd).is_some() { 1 } else { 0 },
    );
    EnableWindow(
        GetDlgItem(hwnd, DELETE),
        if selected(hwnd).is_some() { 1 } else { 0 },
    );
}
unsafe fn refresh_status(hwnd: HWND, s: &State) {
    let dialog = s.dialog.borrow().clone();
    if let Dialog::Manager { ids } = dialog {
        let current = tasks();
        if current.iter().map(|task| &task.id).ne(ids.iter()) {
            refresh_manager(hwnd, s, selected(hwnd));
            return;
        }
        for (row, task) in current.iter().enumerate() {
            let summary = task.summary();
            let mut buffer = [0u16; 256];
            let mut item: LVITEMW = mem::zeroed();
            item.iSubItem = 1;
            item.pszText = buffer.as_mut_ptr();
            item.cchTextMax = buffer.len() as i32;
            let length = SendMessageW(
                GetDlgItem(hwnd, LIST),
                LVM_GETITEMTEXTW,
                row,
                &mut item as *mut _ as LPARAM,
            )
            .max(0) as usize;
            if String::from_utf16_lossy(&buffer[..length.min(buffer.len())]) != summary {
                let mut value = to_wide(&summary);
                item.pszText = value.as_mut_ptr();
                SendMessageW(
                    GetDlgItem(hwnd, LIST),
                    LVM_SETITEMTEXTW,
                    row,
                    &item as *const _ as LPARAM,
                );
            }
        }
    }
}

unsafe fn refresh_batch(hwnd: HWND, s: &State, selection: Option<usize>) {
    let state = s.dialog.borrow().clone();
    if let Dialog::Batch { steps, .. } = state {
        rows(
            hwnd,
            steps
                .iter()
                .map(|step| match step {
                    Step::Launch { command } => vec![
                        command.link.clone(),
                        command.args.clone(),
                        command.work_dir.clone(),
                    ],
                    Step::Delay { milliseconds } => vec![
                        format!("延时 {milliseconds} 毫秒"),
                        String::new(),
                        String::new(),
                    ],
                })
                .collect(),
            selection,
        );
    }
}
unsafe fn init(hwnd: HWND, s: &State, dialog: &Dialog) {
    let mut area = RECT {
        left: 0,
        top: 0,
        right: s.size.0,
        bottom: s.size.1,
    };
    AdjustWindowRectEx(&mut area, GetWindowLongW(hwnd, GWL_STYLE) as u32, 0, 0);
    let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = mem::zeroed();
    info.cbSize = mem::size_of_val(&info) as u32;
    GetMonitorInfoW(monitor, &mut info);
    let work = info.rcWork;
    SetWindowPos(
        hwnd,
        0,
        work.left + (work.right - work.left - (area.right - area.left)) / 2,
        work.top + (work.bottom - work.top - (area.bottom - area.top)) / 2,
        area.right - area.left,
        area.bottom - area.top,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    match dialog {
        Dialog::Manager { .. } => {
            ctrl(
                hwnd,
                s,
                "STATIC",
                "",
                1699,
                14,
                10,
                512,
                34,
                SS_ETCHEDFRAME as u32,
                0,
            );
            for (index, (id, title)) in [
                (ADD, "添加"),
                (DELETE, "删除"),
                (MODIFY, "修改"),
                (IDCANCEL, "退出"),
            ]
            .iter()
            .enumerate()
            {
                tool(hwnd, s, title, *id, 27 + index as i32 * 70, 14, 65);
            }
            list(
                hwnd,
                s,
                14,
                58,
                512,
                424,
                &[("执行类型", 130), ("执行时间", 120), ("任务名称", 258)],
                false,
            );
            refresh_manager(hwnd, s, None);
            SetTimer(hwnd, MANAGER_TIMER, 1000, None);
        }
        Dialog::Editor { task, batch } => {
            field(hwnd, s, "名称：", NAME, &task.name, 10, 205);
            button(hwnd, s, "添加批", BATCH, 264, 10, 48);
            tool(hwnd, s, "提醒", REMIND, 316, 8, 57);
            tool(hwnd, s, "", FILE, 376, 8, 23);
            tool(hwnd, s, "", FOLDER, 400, 8, 23);
            tool(hwnd, s, "", SYSTEM, 424, 8, 23);
            tool(hwnd, s, "", SHORTCUT, 448, 8, 23);
            let command = task
                .steps
                .first()
                .and_then(|step| {
                    if let Step::Launch { command } = step {
                        Some(command.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            field(
                hwnd,
                s,
                "链接：",
                LINK,
                if *batch { "[批处理]" } else { &command.link },
                42,
                320,
            );
            button(hwnd, s, "测试", TEST, 378, 42, 42);
            button(hwnd, s, "转换", CONVERT, 425, 42, 42);
            field(hwnd, s, "参数：", ARGS, &command.args, 74, 417);
            field(hwnd, s, "起始：", START, &command.work_dir, 106, 417);
            ctrl(
                hwnd,
                s,
                "BUTTON",
                "执行类型",
                -1,
                8,
                154,
                258,
                220,
                BS_GROUPBOX as u32,
                0,
            );
            ctrl(
                hwnd,
                s,
                "BUTTON",
                "执行时间",
                -1,
                274,
                154,
                193,
                220,
                BS_GROUPBOX as u32,
                0,
            );
            let positions = [
                (Trigger::Daily, 0, 0),
                (Trigger::Monthly, 0, 1),
                (Trigger::Once, 0, 2),
                (Trigger::Interval, 0, 3),
                (Trigger::Startup, 0, 4),
                (Trigger::NextStartup, 0, 5),
                (Trigger::Exit, 0, 6),
                (Trigger::ThisExit, 0, 7),
                (Trigger::Weekly, 1, 0),
                (Trigger::Yearly, 1, 1),
                (Trigger::NetworkUp, 1, 2),
                (Trigger::NetworkDown, 1, 3),
                (Trigger::Countdown, 1, 4),
            ];
            for (trigger, column, row) in positions {
                let index = Trigger::ALL.iter().position(|t| *t == trigger).unwrap();
                let control = ctrl(
                    hwnd,
                    s,
                    "BUTTON",
                    trigger.label(),
                    RADIO + index as i32,
                    24 + column * 136,
                    178 + row * 24,
                    if column == 0 { 134 } else { 100 },
                    20,
                    WS_TABSTOP | BS_RADIOBUTTON as u32,
                    0,
                );
                SendMessageW(
                    control,
                    BM_SETCHECK,
                    usize::from(task.trigger == trigger),
                    0,
                );
            }
            ctrl(
                hwnd,
                s,
                "STATIC",
                "时间：",
                TIME_LABEL,
                289,
                179,
                40,
                18,
                0,
                0,
            );
            let time = ctrl(
                hwnd,
                s,
                "SysDateTimePick32",
                "",
                TIME,
                330,
                177,
                96,
                20,
                WS_TABSTOP | DTS_TIMEFORMAT | DTS_UPDOWN,
                0,
            );
            SendMessageW(
                time,
                DTM_SETFORMATW,
                0,
                to_wide("HH:mm:ss").as_ptr() as LPARAM,
            );
            ctrl(
                hwnd,
                s,
                "BUTTON",
                "",
                ENABLED,
                435,
                179,
                18,
                18,
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                0,
            );
            SendMessageW(
                GetDlgItem(hwnd, ENABLED),
                BM_SETCHECK,
                usize::from(task.enabled),
                0,
            );
            let month = ctrl(
                hwnd,
                s,
                "COMBOBOX",
                "",
                MONTH,
                289,
                202,
                80,
                220,
                WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
                0,
            );
            for name in [
                "一月",
                "二月",
                "三月",
                "四月",
                "五月",
                "六月",
                "七月",
                "八月",
                "九月",
                "十月",
                "十一月",
                "十二月",
            ] {
                SendMessageW(month, CB_ADDSTRING, 0, to_wide(name).as_ptr() as LPARAM);
            }
            let year = ctrl(
                hwnd,
                s,
                "COMBOBOX",
                "",
                YEAR,
                378,
                202,
                73,
                240,
                WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
                0,
            );
            for year in 1900..=2200 {
                SendMessageW(
                    GetDlgItem(hwnd, YEAR),
                    CB_ADDSTRING,
                    0,
                    to_wide(&year.to_string()).as_ptr() as LPARAM,
                );
            }
            let _ = year;
            for (index, title) in ["日", "一", "二", "三", "四", "五", "六"]
                .iter()
                .enumerate()
            {
                ctrl(
                    hwnd,
                    s,
                    "STATIC",
                    title,
                    -1,
                    289 + index as i32 * 23,
                    229,
                    23,
                    17,
                    SS_CENTER as u32,
                    0,
                );
            }
            for index in 0..42 {
                ctrl(
                    hwnd,
                    s,
                    "BUTTON",
                    "",
                    DAY + index,
                    289 + index % 7 * 23,
                    246 + index / 7 * 17,
                    23,
                    17,
                    BS_OWNERDRAW as u32,
                    0,
                );
            }
            button(hwnd, s, "确定", IDOK, 314, 388, 73);
            button(hwnd, s, "取消", IDCANCEL, 394, 388, 73);
            update_editor(hwnd, s, task, *batch, true);
            SetFocus(GetDlgItem(hwnd, NAME));
        }
        Dialog::Batch { .. } => {
            ctrl(
                hwnd,
                s,
                "BUTTON",
                "添加到列表",
                -1,
                18,
                16,
                368,
                116,
                BS_GROUPBOX as u32,
                0,
            );
            label(hwnd, s, "链接：", 40, 42, 40);
            ctrl(
                hwnd,
                s,
                "EDIT",
                "",
                LINK,
                82,
                40,
                167,
                20,
                WS_TABSTOP | ES_AUTOHSCROLL as u32,
                WS_EX_CLIENTEDGE,
            );
            tool(hwnd, s, "", CONVERT, 255, 38, 23);
            for (index, id) in [FILE, FOLDER, SYSTEM, SHORTCUT].iter().enumerate() {
                tool(hwnd, s, "", *id, 280 + index as i32 * 25, 38, 23);
            }
            label(hwnd, s, "参数：", 40, 70, 40);
            ctrl(
                hwnd,
                s,
                "EDIT",
                "",
                ARGS,
                82,
                68,
                291,
                20,
                WS_TABSTOP | ES_AUTOHSCROLL as u32,
                WS_EX_CLIENTEDGE,
            );
            label(hwnd, s, "起始：", 40, 98, 40);
            ctrl(
                hwnd,
                s,
                "EDIT",
                "",
                START,
                82,
                96,
                215,
                20,
                WS_TABSTOP | ES_AUTOHSCROLL as u32,
                WS_EX_CLIENTEDGE,
            );
            button(hwnd, s, "添加(&A)", ADD_STEP, 305, 96, 68);
            button(hwnd, s, "删除", DELETE, 18, 140, 40);
            button(hwnd, s, "上移", UP, 62, 140, 40);
            button(hwnd, s, "下移", DOWN, 106, 140, 40);
            label(hwnd, s, "延时(毫秒)：", 184, 144, 78);
            ctrl(
                hwnd,
                s,
                "EDIT",
                "1000",
                DELAY,
                260,
                140,
                70,
                20,
                WS_TABSTOP | ES_NUMBER as u32,
                WS_EX_CLIENTEDGE,
            );
            button(hwnd, s, "添加", ADD_DELAY, 338, 140, 48);
            list(
                hwnd,
                s,
                18,
                166,
                368,
                247,
                &[("链接", 200), ("参数", 70), ("起始", 70)],
                true,
            );
            button(hwnd, s, "确定", IDOK, 234, 426, 73);
            button(hwnd, s, "取消", IDCANCEL, 314, 426, 73);
            refresh_batch(hwnd, s, None);
        }
        Dialog::Reminder { text, options } => {
            label(hwnd, s, "提醒内容(不超过500字)：", 18, 24, 340);
            let edit = ctrl(
                hwnd,
                s,
                "EDIT",
                text,
                BODY,
                18,
                52,
                360,
                160,
                WS_TABSTOP
                    | WS_VSCROLL
                    | ES_MULTILINE as u32
                    | ES_AUTOVSCROLL as u32
                    | ES_WANTRETURN as u32,
                WS_EX_CLIENTEDGE,
            );
            SendMessageW(edit, EM_LIMITTEXT, 500, 0);
            ctrl(
                hwnd,
                s,
                "BUTTON",
                "允许自动关闭",
                AUTO_CLOSE,
                18,
                229,
                140,
                20,
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                0,
            );
            SendMessageW(
                GetDlgItem(hwnd, AUTO_CLOSE),
                BM_SETCHECK,
                usize::from(options.allow_auto_close),
                0,
            );
            ctrl(
                hwnd,
                s,
                "BUTTON",
                "播放声音",
                SOUND_ENABLED,
                18,
                261,
                74,
                20,
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                0,
            );
            SendMessageW(
                GetDlgItem(hwnd, SOUND_ENABLED),
                BM_SETCHECK,
                usize::from(options.play_sound),
                0,
            );
            ctrl(
                hwnd,
                s,
                "EDIT",
                &options.sound_file,
                SOUND_FILE,
                91,
                260,
                237,
                20,
                WS_TABSTOP | ES_AUTOHSCROLL as u32,
                WS_EX_CLIENTEDGE,
            );
            tool(hwnd, s, "", SOUND_PICK, 333, 257, 23);
            tool(hwnd, s, "", SOUND_PREVIEW, 358, 257, 23);
            button(hwnd, s, "确定", IDOK, 226, 300, 73);
            button(hwnd, s, "取消", IDCANCEL, 306, 300, 73);
            SetFocus(edit);
        }
    }
}
unsafe fn update_editor(hwnd: HWND, s: &State, task: &Task, batch: bool, set_time: bool) {
    for (index, trigger) in Trigger::ALL.iter().enumerate() {
        SendMessageW(
            GetDlgItem(hwnd, RADIO + index as i32),
            BM_SETCHECK,
            usize::from(*trigger == task.trigger),
            0,
        );
    }
    set_ctrl_text(
        hwnd,
        TIME_LABEL,
        if task.trigger.duration() {
            "时长："
        } else {
            "时间："
        },
    );
    EnableWindow(
        GetDlgItem(hwnd, TIME),
        if task.trigger.calendar() || task.trigger.duration() {
            1
        } else {
            0
        },
    );
    for id in [LINK, ARGS, START] {
        SendMessageW(GetDlgItem(hwnd, id), EM_SETREADONLY, usize::from(batch), 0);
    }
    if batch {
        set_ctrl_text(
            hwnd,
            LINK,
            &format!("[批处理：{} 个步骤]", task.steps.len()),
        );
        set_ctrl_text(hwnd, ARGS, "");
        set_ctrl_text(hwnd, START, "");
    }
    if set_time {
        let time = if task.trigger.duration() {
            NaiveTime::from_num_seconds_from_midnight_opt(task.seconds.min(86399), 0)
        } else {
            NaiveTime::parse_from_str(&task.time, "%H:%M:%S").ok()
        }
        .unwrap_or_default();
        let mut value: SYSTEMTIME = mem::zeroed();
        value.wYear = 2026;
        value.wMonth = 1;
        value.wDay = 1;
        value.wHour = time.hour() as u16;
        value.wMinute = time.minute() as u16;
        value.wSecond = time.second() as u16;
        SendMessageW(
            GetDlgItem(hwnd, TIME),
            DTM_SETSYSTEMTIME,
            GDT_VALID as usize,
            &value as *const _ as LPARAM,
        );
    }
    calendar(hwnd, s, task);
}
unsafe fn calendar(hwnd: HWND, _s: &State, task: &Task) {
    let date = NaiveDate::parse_from_str(&task.date, "%Y-%m-%d")
        .unwrap_or_else(|_| crate::local_clock::now().date_naive());
    SendMessageW(
        GetDlgItem(hwnd, MONTH),
        CB_SETCURSEL,
        date.month0() as usize,
        0,
    );
    SendMessageW(
        GetDlgItem(hwnd, YEAR),
        CB_SETCURSEL,
        (date.year() - 1900).clamp(0, 300) as usize,
        0,
    );
    let first = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap();
    let offset = first.weekday().num_days_from_sunday() as i32;
    for index in 0..42 {
        let day = index - offset + 1;
        let value = if day > 0
            && NaiveDate::from_ymd_opt(date.year(), date.month(), day as u32).is_some()
        {
            day.to_string()
        } else {
            String::new()
        };
        set_ctrl_text(hwnd, DAY + index, &value);
        EnableWindow(
            GetDlgItem(hwnd, DAY + index),
            if task.trigger.calendar() && !value.is_empty() {
                1
            } else {
                0
            },
        );
        InvalidateRect(GetDlgItem(hwnd, DAY + index), ptr::null(), 1);
    }
    for id in [MONTH, YEAR] {
        EnableWindow(
            GetDlgItem(hwnd, id),
            if task.trigger.calendar() { 1 } else { 0 },
        );
    }
}
unsafe fn command_fields(hwnd: HWND) -> Command {
    Command {
        link: get_ctrl_text(hwnd, LINK).trim().into(),
        args: get_ctrl_text(hwnd, ARGS),
        work_dir: get_ctrl_text(hwnd, START).trim().into(),
    }
}
unsafe fn read_task(hwnd: HWND, task: &Task, batch: bool) -> Result<Task, String> {
    let mut task = task.clone();
    task.name = get_ctrl_text(hwnd, NAME).trim().into();
    task.enabled = SendMessageW(GetDlgItem(hwnd, ENABLED), BM_GETCHECK, 0, 0) != 0;
    let mut value: SYSTEMTIME = mem::zeroed();
    if SendMessageW(
        GetDlgItem(hwnd, TIME),
        DTM_GETSYSTEMTIME,
        0,
        &mut value as *mut _ as LPARAM,
    ) != GDT_VALID as isize
    {
        return Err("请输入有效时间。".into());
    }
    if task.trigger.duration() {
        task.seconds = u32::from(value.wHour) * 3600
            + u32::from(value.wMinute) * 60
            + u32::from(value.wSecond);
    } else {
        task.time = format!(
            "{:02}:{:02}:{:02}",
            value.wHour, value.wMinute, value.wSecond
        );
    }
    if !batch {
        let command = command_fields(hwnd);
        task.steps = if command.link.is_empty() {
            Vec::new()
        } else {
            vec![Step::Launch { command }]
        };
    }
    task.validate()?;
    Ok(task)
}
unsafe fn edit(owner: HWND, base: &std::path::Path, task: Task) {
    let Some((main, _, _)) = snapshot() else {
        return;
    };
    let batch = task.steps.len() > 1 || matches!(task.steps.first(), Some(Step::Delay { .. }));
    show(
        owner,
        main,
        base.to_owned(),
        "设置计划任务",
        (476, 416),
        Dialog::Editor { task, batch },
    );
}

unsafe fn set_command(hwnd: HWND, s: &State, command: Command, name: &str) {
    if let Ok(mut state) = s.dialog.try_borrow_mut() {
        if let Dialog::Editor { task, batch } = &mut *state {
            *batch = false;
            task.steps = vec![Step::Launch {
                command: command.clone(),
            }];
        }
    }
    for id in [LINK, ARGS, START] {
        SendMessageW(GetDlgItem(hwnd, id), EM_SETREADONLY, 0, 0);
    }
    set_ctrl_text(hwnd, LINK, &command.link);
    set_ctrl_text(hwnd, ARGS, &command.args);
    set_ctrl_text(hwnd, START, &command.work_dir);
    if GetDlgItem(hwnd, NAME) != 0 && get_ctrl_text(hwnd, NAME).trim().is_empty() {
        set_ctrl_text(hwnd, NAME, name);
    }
}
unsafe fn choose_command(hwnd: HWND, s: &State, id: i32) {
    let chosen = match id {
        FILE => crate::pick_file_dialog(hwnd, "选择文件", &[("所有文件", "*.*")]).map(|path| {
            let settings = APP_STATE
                .with(|cell| {
                    cell.try_borrow()
                        .ok()
                        .and_then(|guard| guard.as_ref().map(|app| app.user_cfg.advanced.clone()))
                })
                .unwrap_or_default();
            let file = crate::advanced_runtime::prepare_drop(&s.base, &settings, &path);
            (
                file.name,
                Command {
                    link: file.target,
                    args: file.args,
                    work_dir: file.work_dir,
                },
            )
        }),
        FOLDER => crate::pick_folder_dialog(hwnd, "选择文件夹").map(|path| {
            (
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                Command {
                    link: path.to_string_lossy().into_owned(),
                    ..Command::default()
                },
            )
        }),
        SYSTEM => crate::hotkey_dialog::choose_system(hwnd).map(|choice| {
            (
                choice.label.into(),
                Command {
                    link: choice.target.into(),
                    args: choice.args.into(),
                    work_dir: String::new(),
                },
            )
        }),
        SHORTCUT => {
            let choices = APP_STATE
                .with(|cell| {
                    let guard = cell.try_borrow().ok()?;
                    Some(
                        guard
                            .as_ref()?
                            .categories
                            .iter()
                            .flat_map(|cat| &cat.groups)
                            .flat_map(|group| &group.items)
                            .map(|item| {
                                (
                                    item.name.clone(),
                                    Command {
                                        link: item.target_path.clone(),
                                        args: item.args.clone(),
                                        work_dir: item.work_dir.clone(),
                                    },
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .unwrap_or_default();
            if choices.is_empty() {
                crate::advanced_runtime::message(hwnd, "当前没有可选择的快捷方式。");
                return;
            }
            let labels = choices
                .iter()
                .map(|(name, _)| (name.as_str(), false))
                .collect::<Vec<_>>();
            crate::popup::choose(hwnd, GetDlgItem(hwnd, id), &labels, 240)
                .and_then(|index| choices.get(index).cloned())
        }
        _ => None,
    };
    if let Some((name, command)) = chosen {
        set_command(hwnd, s, command, &name);
    }
}
unsafe fn convert(hwnd: HWND, s: &State) {
    let labels = [("转换成相对路径(&X)", false), ("转换成绝对路径(&J)", false)];
    let Some(choice) = crate::popup::choose(hwnd, GetDlgItem(hwnd, CONVERT), &labels, 188) else {
        return;
    };
    let mode = if choice == 0 {
        crate::targets::PathMode::Relative
    } else {
        crate::targets::PathMode::Absolute
    };
    let convert_command = |command: &mut Command| -> Result<(), String> {
        if matches!(
            crate::targets::TargetKind::infer(&s.base, &command.link),
            crate::targets::TargetKind::File | crate::targets::TargetKind::Folder
        ) {
            command.link = crate::targets::convert_path(&s.base, &command.link, mode)
                .ok_or("链接路径无法转换，请检查驱动器及路径。")?;
        }
        if !command.work_dir.is_empty() {
            command.work_dir = crate::targets::convert_path(&s.base, &command.work_dir, mode)
                .ok_or("起始目录无法转换。")?;
        }
        Ok(())
    };
    let state = s.dialog.borrow().clone();
    if let Dialog::Editor {
        mut task,
        batch: true,
    } = state
    {
        for step in &mut task.steps {
            if let Step::Launch { command } = step {
                if let Err(error) = convert_command(command) {
                    crate::advanced_runtime::message(hwnd, &error);
                    return;
                }
            }
        }
        if let Ok(mut state) = s.dialog.try_borrow_mut() {
            *state = Dialog::Editor { task, batch: true };
        }
    } else {
        let mut command = command_fields(hwnd);
        match convert_command(&mut command) {
            Ok(()) => {
                set_ctrl_text(hwnd, LINK, &command.link);
                set_ctrl_text(hwnd, START, &command.work_dir);
            }
            Err(error) => crate::advanced_runtime::message(hwnd, &error),
        }
    }
}
unsafe fn activate(hwnd: HWND, s: &State, id: i32, notification: u32) {
    let dialog = s.dialog.borrow().clone();
    if id == IDCANCEL {
        EndDialog(hwnd, IDCANCEL as isize);
        return;
    }
    if [FILE, FOLDER, SYSTEM, SHORTCUT].contains(&id) {
        choose_command(hwnd, s, id);
        return;
    }
    if id == CONVERT {
        convert(hwnd, s);
        return;
    }
    match dialog {
        Dialog::Manager { ids } => {
            let row = selected(hwnd);
            let existing = row
                .and_then(|row| ids.get(row))
                .and_then(|id| tasks().into_iter().find(|task| task.id == *id));
            match id {
                ADD => {
                    if let Some((_, _, session)) = snapshot() {
                        edit(
                            hwnd,
                            &s.base,
                            Task::new(crate::local_clock::now(), &session),
                        );
                        refresh_manager(hwnd, s, Some(tasks().len().saturating_sub(1)));
                    }
                }
                MODIFY | IDOK => {
                    if let Some(task) = existing {
                        edit(hwnd, &s.base, task);
                        refresh_manager(hwnd, s, row);
                    }
                }
                DELETE => {
                    if let Some(task) = existing {
                        if crate::advanced_runtime::confirm(
                            hwnd,
                            &format!("删除计划任务“{}”？", task.name),
                        ) {
                            if let Err(error) = commit(&s.base, None, Some(&task.id)) {
                                crate::advanced_runtime::message(hwnd, &error);
                            }
                            refresh_manager(
                                hwnd,
                                s,
                                row.map(|row| row.min(tasks().len().saturating_sub(1))),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        Dialog::Editor { mut task, batch } => {
            if (RADIO..RADIO + Trigger::ALL.len() as i32).contains(&id) {
                let mut value: SYSTEMTIME = mem::zeroed();
                SendMessageW(
                    GetDlgItem(hwnd, TIME),
                    DTM_GETSYSTEMTIME,
                    0,
                    &mut value as *mut _ as LPARAM,
                );
                if task.trigger.duration() {
                    task.seconds = u32::from(value.wHour) * 3600
                        + u32::from(value.wMinute) * 60
                        + u32::from(value.wSecond);
                } else {
                    task.time = format!(
                        "{:02}:{:02}:{:02}",
                        value.wHour, value.wMinute, value.wSecond
                    );
                }
                task.trigger = Trigger::ALL[(id - RADIO) as usize];
                if let Ok(mut state) = s.dialog.try_borrow_mut() {
                    *state = Dialog::Editor {
                        task: task.clone(),
                        batch,
                    };
                }
                update_editor(hwnd, s, &task, batch, true);
                return;
            }
            if ((id == MONTH || id == YEAR) && notification == CBN_SELCHANGE)
                || (DAY..DAY + 42).contains(&id)
            {
                let old = NaiveDate::parse_from_str(&task.date, "%Y-%m-%d")
                    .unwrap_or_else(|_| crate::local_clock::now().date_naive());
                let month =
                    SendMessageW(GetDlgItem(hwnd, MONTH), CB_GETCURSEL, 0, 0).max(0) as u32 + 1;
                let year =
                    1900 + SendMessageW(GetDlgItem(hwnd, YEAR), CB_GETCURSEL, 0, 0).max(0) as i32;
                let mut day = if (DAY..DAY + 42).contains(&id) {
                    let Ok(day) = get_ctrl_text(hwnd, id).parse::<u32>() else {
                        return;
                    };
                    day
                } else {
                    old.day()
                };
                while NaiveDate::from_ymd_opt(year, month, day).is_none() && day > 1 {
                    day -= 1;
                }
                if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                    task.date = date.format("%Y-%m-%d").to_string();
                    if let Ok(mut state) = s.dialog.try_borrow_mut() {
                        *state = Dialog::Editor {
                            task: task.clone(),
                            batch,
                        };
                    }
                    calendar(hwnd, s, &task);
                }
                return;
            }
            match id {
                BATCH => {
                    let steps = if batch {
                        task.steps.clone()
                    } else {
                        let command = command_fields(hwnd);
                        if command.link.is_empty() {
                            Vec::new()
                        } else {
                            vec![Step::Launch { command }]
                        }
                    };
                    if let Some(Dialog::Batch { steps, .. }) = show(
                        hwnd,
                        s.main,
                        s.base.clone(),
                        "添加批处理",
                        (402, 456),
                        Dialog::Batch {
                            steps,
                            editing: None,
                        },
                    ) {
                        task.steps = steps;
                        let batch = !task.steps.is_empty();
                        if let Ok(mut state) = s.dialog.try_borrow_mut() {
                            *state = Dialog::Editor {
                                task: task.clone(),
                                batch,
                            };
                        }
                        update_editor(hwnd, s, &task, batch, false);
                        if !batch {
                            set_command(hwnd, s, Command::default(), "");
                        }
                    }
                }
                REMIND => {
                    if let Some(Dialog::Reminder { text, options }) = show(
                        hwnd,
                        s.main,
                        s.base.clone(),
                        "增加提醒",
                        (398, 334),
                        Dialog::Reminder {
                            text: task.reminder.clone(),
                            options: task.reminder_options.clone(),
                        },
                    ) {
                        task.reminder = text;
                        task.reminder_options = options;
                        if let Ok(mut state) = s.dialog.try_borrow_mut() {
                            *state = Dialog::Editor { task, batch };
                        }
                    }
                }
                TEST => {
                    if get_ctrl_text(hwnd, NAME).trim().is_empty() {
                        set_ctrl_text(hwnd, NAME, "测试任务");
                    }
                    match read_task(hwnd, &task, batch) {
                        Ok(task) => crate::schedule_runtime::test(s.main, task),
                        Err(error) => crate::advanced_runtime::message(hwnd, &error),
                    }
                }
                IDOK => match read_task(hwnd, &task, batch) {
                    Ok(mut task) => {
                        if let Some((_, _, session)) = snapshot() {
                            task.arm(crate::local_clock::now(), &session);
                            match commit(&s.base, Some(task.clone()), None) {
                                Ok(()) => {
                                    if let Ok(mut state) = s.dialog.try_borrow_mut() {
                                        *state = Dialog::Editor { task, batch };
                                    }
                                    EndDialog(hwnd, IDOK as isize);
                                }
                                Err(error) => crate::advanced_runtime::message(hwnd, &error),
                            }
                        }
                    }
                    Err(error) => crate::advanced_runtime::message(hwnd, &error),
                },
                _ => {}
            }
        }
        Dialog::Batch {
            mut steps,
            mut editing,
        } => {
            let row = selected(hwnd);
            let mut selection = row;
            match id {
                ADD_STEP => {
                    let command = command_fields(hwnd);
                    if command.link.is_empty() {
                        crate::advanced_runtime::message(hwnd, "请输入链接。");
                        return;
                    }
                    let step = Step::Launch { command };
                    if let Some(index) = editing.filter(|index| *index < steps.len()) {
                        steps[index] = step;
                        selection = Some(index);
                    } else {
                        steps.push(step);
                        selection = Some(steps.len() - 1);
                    }
                    editing = None;
                    set_ctrl_text(hwnd, LINK, "");
                    set_ctrl_text(hwnd, ARGS, "");
                    set_ctrl_text(hwnd, START, "");
                }
                ADD_DELAY => {
                    let Ok(milliseconds) = get_ctrl_text(hwnd, DELAY).parse::<u32>() else {
                        crate::advanced_runtime::message(hwnd, "请输入 1 至 86400000 毫秒的延时。");
                        return;
                    };
                    if !(1..=86_400_000).contains(&milliseconds) {
                        crate::advanced_runtime::message(hwnd, "延时必须为 1 至 86400000 毫秒。");
                        return;
                    }
                    let step = Step::Delay { milliseconds };
                    if let Some(index) = editing.filter(|index| *index < steps.len()) {
                        steps[index] = step;
                        selection = Some(index);
                    } else {
                        steps.push(step);
                        selection = Some(steps.len() - 1);
                    }
                    editing = None;
                }
                DELETE => {
                    if let Some(row) = row.filter(|row| *row < steps.len()) {
                        steps.remove(row);
                        selection = Some(row.min(steps.len().saturating_sub(1)));
                        editing = None;
                    }
                }
                UP | DOWN => {
                    if let Some(row) = row {
                        let destination = if id == UP {
                            row.checked_sub(1)
                        } else {
                            row.checked_add(1).filter(|row| *row < steps.len())
                        };
                        if let Some(destination) = destination {
                            steps.swap(row, destination);
                            selection = Some(destination);
                            editing = None;
                        }
                    }
                }
                MODIFY => {
                    if let Some(row) = row {
                        if let Some(Step::Launch { command }) = steps.get(row) {
                            set_command(hwnd, s, command.clone(), "");
                            editing = Some(row);
                        } else if let Some(Step::Delay { milliseconds }) = steps.get(row) {
                            set_ctrl_text(hwnd, DELAY, &milliseconds.to_string());
                            editing = Some(row);
                        }
                    }
                }
                IDOK => {
                    if steps.len() > 1000 {
                        crate::advanced_runtime::message(hwnd, "批处理最多 1000 个步骤。");
                        return;
                    }
                    if let Ok(mut state) = s.dialog.try_borrow_mut() {
                        *state = Dialog::Batch {
                            steps,
                            editing: None,
                        };
                    }
                    EndDialog(hwnd, IDOK as isize);
                    return;
                }
                _ => return,
            }
            if let Ok(mut state) = s.dialog.try_borrow_mut() {
                *state = Dialog::Batch { steps, editing };
            }
            refresh_batch(hwnd, s, selection);
        }
        Dialog::Reminder { .. } => match id {
            SOUND_PICK => {
                if let Some(path) = crate::pick_file_dialog(
                    hwnd,
                    "选择提醒声音",
                    &[
                        ("声音文件", "*.wav;*.mp3;*.wma;*.mid;*.midi"),
                        ("所有文件", "*.*"),
                    ],
                ) {
                    s.preview.replace(None);
                    set_ctrl_text(hwnd, SOUND_FILE, &path.to_string_lossy());
                    SendMessageW(GetDlgItem(hwnd, SOUND_ENABLED), BM_SETCHECK, 1, 0);
                }
            }
            SOUND_PREVIEW => {
                s.preview.replace(None);
                match crate::schedule_reminder::Playback::start(
                    &s.base,
                    &get_ctrl_text(hwnd, SOUND_FILE),
                ) {
                    Ok(player) => {
                        s.preview.replace(Some(player));
                    }
                    Err(error) => crate::advanced_runtime::message(hwnd, &error),
                }
            }
            SOUND_ENABLED
                if SendMessageW(GetDlgItem(hwnd, SOUND_ENABLED), BM_GETCHECK, 0, 0) == 0 =>
            {
                s.preview.replace(None);
            }
            IDOK => {
                let text = get_ctrl_text(hwnd, BODY);
                let options = ReminderOptions {
                    allow_auto_close: SendMessageW(GetDlgItem(hwnd, AUTO_CLOSE), BM_GETCHECK, 0, 0)
                        != 0,
                    play_sound: SendMessageW(GetDlgItem(hwnd, SOUND_ENABLED), BM_GETCHECK, 0, 0)
                        != 0,
                    sound_file: get_ctrl_text(hwnd, SOUND_FILE).trim().into(),
                };
                if let Err(error) = options.validate(&text) {
                    crate::advanced_runtime::message(hwnd, &error);
                    return;
                }
                if options.play_sound
                    && !crate::targets::resolve_path(&s.base, &options.sound_file).is_file()
                {
                    crate::advanced_runtime::message(hwnd, "声音文件不存在，请重新选择。");
                    return;
                }
                if let Ok(mut state) = s.dialog.try_borrow_mut() {
                    *state = Dialog::Reminder { text, options };
                }
                EndDialog(hwnd, IDOK as isize);
            }
            _ => {}
        },
    }
}
unsafe fn draw_sound_button(dc: HDC, mut area: RECT, preview: bool, pressed: bool) {
    FillRect(dc, &area, GetSysColorBrush(COLOR_BTNFACE));
    if pressed {
        DrawEdge(dc, &mut area, BDR_SUNKENOUTER, BF_RECT);
    }
    let brush = CreateSolidBrush(if preview { 0x00ee9933 } else { 0x00808080 });
    let pen = CreatePen(PS_SOLID, 1, if preview { 0x00aa6622 } else { 0x00505050 });
    let old_brush = SelectObject(dc, brush);
    let old_pen = SelectObject(dc, pen);
    let (x, y) = (area.left, area.top);
    let points: Vec<POINT> = if preview {
        vec![
            POINT { x: x + 8, y: y + 6 },
            POINT {
                x: x + 8,
                y: y + 18,
            },
            POINT {
                x: x + 15,
                y: y + 12,
            },
        ]
    } else {
        vec![
            POINT {
                x: x + 3,
                y: y + 10,
            },
            POINT {
                x: x + 7,
                y: y + 10,
            },
            POINT {
                x: x + 11,
                y: y + 7,
            },
            POINT {
                x: x + 11,
                y: y + 18,
            },
            POINT {
                x: x + 7,
                y: y + 15,
            },
            POINT {
                x: x + 3,
                y: y + 15,
            },
        ]
    };
    Polygon(dc, points.as_ptr(), points.len() as i32);
    SelectObject(dc, old_pen);
    SelectObject(dc, old_brush);
    DeleteObject(pen);
    DeleteObject(brush);
    if !preview {
        let green = CreateSolidBrush(0x003cae3c);
        let plus = RECT {
            left: x + 13,
            top: y + 12,
            right: x + 21,
            bottom: y + 20,
        };
        FillRect(dc, &plus, green);
        DeleteObject(green);
        let white = GetStockObject(WHITE_BRUSH) as HBRUSH;
        FillRect(
            dc,
            &RECT {
                left: x + 16,
                top: y + 13,
                right: x + 18,
                bottom: y + 19,
            },
            white,
        );
        FillRect(
            dc,
            &RECT {
                left: x + 14,
                top: y + 15,
                right: x + 20,
                bottom: y + 17,
            },
            white,
        );
    }
}

unsafe fn draw(hwnd: HWND, s: &State, d: &DRAWITEMSTRUCT) {
    let mut area = d.rcItem;
    let old_font = SelectObject(d.hDC, s.font);
    let old_mode = SetBkMode(d.hDC, TRANSPARENT as i32);
    let old_color = GetTextColor(d.hDC);
    let id = d.CtlID as i32;
    let title = get_ctrl_text(hwnd, id);
    if id == SOUND_PICK || id == SOUND_PREVIEW {
        draw_sound_button(
            d.hDC,
            area,
            id == SOUND_PREVIEW,
            d.itemState & ODS_SELECTED != 0,
        );
    } else if (DAY..DAY + 42).contains(&id) {
        let selected = match &*s.dialog.borrow() {
            Dialog::Editor { task, .. } => NaiveDate::parse_from_str(&task.date, "%Y-%m-%d")
                .ok()
                .is_some_and(|date| title == date.day().to_string()),
            _ => false,
        };
        FillRect(
            d.hDC,
            &area,
            GetSysColorBrush(if selected {
                COLOR_HIGHLIGHT
            } else {
                COLOR_WINDOW
            }),
        );
        SetTextColor(
            d.hDC,
            GetSysColor(if selected {
                COLOR_HIGHLIGHTTEXT
            } else if d.itemState & ODS_DISABLED != 0 {
                COLOR_GRAYTEXT
            } else {
                COLOR_WINDOWTEXT
            }),
        );
        DrawTextW(
            d.hDC,
            to_wide(&title).as_ptr(),
            -1,
            &mut area,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
    } else {
        FillRect(d.hDC, &area, GetSysColorBrush(COLOR_BTNFACE));
        if d.itemState & (ODS_SELECTED | ODS_FOCUS) != 0 {
            DrawEdge(
                d.hDC,
                &mut area,
                if d.itemState & ODS_SELECTED != 0 {
                    BDR_SUNKENOUTER
                } else {
                    BDR_RAISEDINNER
                },
                BF_RECT,
            );
        }
        let bytes: &[u8] = match id {
            ADD | FILE => include_bytes!("../assets/icons/ctx_add_file.png"),
            DELETE => include_bytes!("../assets/icons/im_delete.png"),
            MODIFY | CONVERT => include_bytes!("../assets/icons/im_edit.png"),
            REMIND => include_bytes!("../assets/icons/im_schedule.png"),
            FOLDER => include_bytes!("../assets/icons/ctx_add_dir.png"),
            SYSTEM => include_bytes!("../assets/icons/ctx_add_sys.png"),
            SHORTCUT => include_bytes!("../assets/icons/im_props.png"),
            _ => include_bytes!("../assets/icons/menu_exit.png"),
        };
        if let Ok(image) = image::load_from_memory(bytes) {
            let image = image.to_rgba8();
            let surface = crate::skin_assets::RgbaSurface::from_pixels(
                image.width(),
                image.height(),
                image.pixels().map(|p| p.0).collect(),
            );
            crate::native_dialogs::blit_surface_on_bg(
                d.hDC,
                &surface,
                area.left + 4,
                area.top + 4,
                16,
                16,
                [240; 3],
            );
        }
        area.left += 25;
        SetTextColor(
            d.hDC,
            GetSysColor(if d.itemState & ODS_DISABLED != 0 {
                COLOR_GRAYTEXT
            } else {
                COLOR_BTNTEXT
            }),
        );
        DrawTextW(
            d.hDC,
            to_wide(&title).as_ptr(),
            -1,
            &mut area,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
    }
    SetTextColor(d.hDC, old_color);
    SetBkMode(d.hDC, old_mode);
    SelectObject(d.hDC, old_font);
}
unsafe fn tooltips(hwnd: HWND) {
    const ITEMS: &[(i32, &str)] = &[
        (TEST, "立即执行计划任务，测试效果"),
        (CONVERT, "转换链接和起始目录的相对或绝对路径"),
        (FILE, "添加文件"),
        (FOLDER, "添加文件夹"),
        (SYSTEM, "添加系统功能"),
        (SHORTCUT, "选择已有快捷方式"),
        (BATCH, "编辑批处理及延时步骤"),
        (REMIND, "设置执行时显示的提醒内容"),
        (AUTO_CLOSE, "提醒显示 60 秒后自动关闭"),
        (SOUND_PICK, "选择提醒声音文件"),
        (SOUND_PREVIEW, "试听所选声音"),
        (ENABLED, "启用或停用此计划任务"),
        (
            TIME,
            "周期任务填写执行时间，间隔及倒计时填写时长（时:分:秒）",
        ),
    ];
    static TEXTS: std::sync::OnceLock<Vec<Vec<u16>>> = std::sync::OnceLock::new();
    let texts = TEXTS.get_or_init(|| ITEMS.iter().map(|(_, text)| to_wide(text)).collect());
    let tooltip = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        to_wide("tooltips_class32").as_ptr(),
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
    if tooltip == 0 {
        return;
    }
    for (index, (id, _)) in ITEMS.iter().enumerate() {
        let control = GetDlgItem(hwnd, *id);
        if control == 0 {
            continue;
        }
        let mut info: TTTOOLINFOW = mem::zeroed();
        info.cbSize = mem::size_of_val(&info) as u32;
        info.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
        info.hwnd = hwnd;
        info.uId = control as usize;
        info.lpszText = texts[index].as_ptr().cast_mut();
        SendMessageW(tooltip, TTM_ADDTOOLW, 0, &info as *const _ as LPARAM);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> isize {
    if message == WM_INITDIALOG {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, lp);
    }
    #[cfg(test)]
    if message == WM_TIMER && crate::dialog_tests::step(hwnd, wp) {
        return 1;
    }
    let Some(s) = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State).as_ref() else {
        return 0;
    };
    match message {
        WM_INITDIALOG => {
            let dialog = s.dialog.borrow().clone();
            init(hwnd, s, &dialog);
            tooltips(hwnd);
            #[cfg(test)]
            crate::dialog_tests::arm(hwnd);
            0
        }
        WM_TIMER if wp == MANAGER_TIMER => {
            refresh_status(hwnd, s);
            1
        }
        WM_CLOSE => {
            EndDialog(hwnd, IDCANCEL as isize);
            1
        }
        WM_COMMAND => {
            activate(hwnd, s, wp as u16 as i32, (wp >> 16) as u32);
            1
        }
        WM_DRAWITEM => {
            if let Some(draw_item) = (lp as *const DRAWITEMSTRUCT).as_ref() {
                draw(hwnd, s, draw_item);
            }
            1
        }
        WM_CTLCOLORDLG | WM_CTLCOLORSTATIC => {
            SetBkColor(wp as HDC, GetSysColor(COLOR_BTNFACE));
            SetTextColor(wp as HDC, GetSysColor(COLOR_BTNTEXT));
            GetSysColorBrush(COLOR_BTNFACE)
        }
        WM_NOTIFY => {
            if let Some(header) = (lp as *const NMHDR).as_ref() {
                if header.idFrom == LIST as usize {
                    if header.code == NM_DBLCLK {
                        activate(hwnd, s, MODIFY, 0);
                    } else if header.code == LVN_ITEMCHANGED {
                        if matches!(&*s.dialog.borrow(), Dialog::Manager { .. }) {
                            let enabled = if selected(hwnd).is_some() { 1 } else { 0 };
                            EnableWindow(GetDlgItem(hwnd, DELETE), enabled);
                            EnableWindow(GetDlgItem(hwnd, MODIFY), enabled);
                        }
                    } else if header.code == LVN_KEYDOWN {
                        let key = &*(lp as *const NMLVKEYDOWN);
                        if key.wVKey == 46 {
                            activate(hwnd, s, DELETE, 0);
                        } else if key.wVKey == 13 {
                            activate(hwnd, s, MODIFY, 0);
                        }
                    }
                }
            }
            0
        }
        WM_SIZE => {
            if matches!(&*s.dialog.borrow(), Dialog::Manager { .. }) {
                let mut area: RECT = mem::zeroed();
                GetClientRect(hwnd, &mut area);
                MoveWindow(
                    GetDlgItem(hwnd, 1699),
                    14,
                    10,
                    (area.right - 28).max(100),
                    34,
                    1,
                );
                MoveWindow(
                    GetDlgItem(hwnd, LIST),
                    14,
                    58,
                    (area.right - 28).max(100),
                    (area.bottom - 76).max(80),
                    1,
                );
            }
            0
        }
        _ => 0,
    }
}

#[cfg(test)]
mod ui_tests {
    use super::*;
    use crate::dialog_tests::{arm, capture, enable, queue};
    fn write_silent_wave(path: &std::path::Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let samples = vec![0u8; 1600];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36u32 + samples.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(samples.len() as u32).to_le_bytes());
        bytes.extend(samples);
        std::fs::write(path, bytes).unwrap();
    }
    unsafe fn pump_until_closed(hwnd: HWND) {
        let mut message: MSG = mem::zeroed();
        while IsWindow(hwnd) != 0 {
            assert!(GetMessageW(&mut message, 0, 0, 0) > 0);
            if IsDialogMessageW(hwnd, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    #[test]
    fn native_schedule_workflows_when_requested() {
        let Some(base) = std::env::var_os("KSTART_PLAN_UI_BASE") else {
            return;
        };
        let base = PathBuf::from(base);
        let shots = base.join("shots");
        write_silent_wave(&base.join("test-reminder.wav"));
        let audio = crate::schedule_reminder::Playback::start(&base, "test-reminder.wav")
            .expect("测试声音应可打开并播放");
        drop(audio);
        std::fs::create_dir_all(&shots).unwrap();
        let app = crate::AppState::new(base.clone(), crate::ActivePopupMenu::None).unwrap();
        let original_data = std::fs::read(base.join("user/data.json")).unwrap();
        APP_STATE.with(|cell| *cell.try_borrow_mut().unwrap() = Some(app));
        enable(true);
        unsafe {
            let controls = INITCOMMONCONTROLSEX {
                dwSize: mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_LISTVIEW_CLASSES | ICC_DATE_CLASSES,
            };
            InitCommonControlsEx(&controls);
            let shot = shots.join("plan-manager-empty.png");
            queue(move |hwnd| {
                capture(hwnd, &shot);
                SendMessageW(hwnd, WM_COMMAND, ADD as usize, 0);
                arm(hwnd);
            });
            let shot = shots.join("plan-editor.png");
            queue(move |hwnd| {
                assert_ne!(GetDlgItem(hwnd, TIME), 0);
                let mut time: SYSTEMTIME = mem::zeroed();
                assert_eq!(
                    SendMessageW(
                        GetDlgItem(hwnd, TIME),
                        DTM_GETSYSTEMTIME,
                        0,
                        &mut time as *mut _ as LPARAM
                    ),
                    GDT_VALID as isize
                );
                set_ctrl_text(hwnd, NAME, "每日备份提醒");
                set_ctrl_text(hwnd, LINK, "notepad.exe");
                capture(hwnd, &shot);
                SendMessageW(hwnd, WM_COMMAND, BATCH as usize, 0);
                arm(hwnd);
            });
            let shot = shots.join("plan-batch.png");
            queue(move |hwnd| {
                set_ctrl_text(hwnd, LINK, "notepad.exe");
                set_ctrl_text(hwnd, ARGS, "first.txt");
                SendMessageW(hwnd, WM_COMMAND, ADD_STEP as usize, 0);
                SendMessageW(hwnd, WM_COMMAND, ADD_DELAY as usize, 0);
                set_ctrl_text(hwnd, LINK, "notepad.exe");
                set_ctrl_text(hwnd, ARGS, "second.txt");
                SendMessageW(hwnd, WM_COMMAND, ADD_STEP as usize, 0);
                assert_eq!(
                    SendMessageW(GetDlgItem(hwnd, LIST), LVM_GETITEMCOUNT, 0, 0),
                    4
                );
                SendMessageW(hwnd, WM_COMMAND, UP as usize, 0);
                SendMessageW(hwnd, WM_COMMAND, DOWN as usize, 0);
                capture(hwnd, &shot);
                SendMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
            });
            queue(|hwnd| {
                SendMessageW(hwnd, WM_COMMAND, REMIND as usize, 0);
                arm(hwnd);
            });
            let shot = shots.join("plan-reminder.png");
            let empty_shot = shots.join("plan-reminder-empty.png");
            queue(move |hwnd| {
                capture(hwnd, &empty_shot);
                assert_eq!(
                    SendMessageW(GetDlgItem(hwnd, BODY), EM_GETLIMITTEXT, 0, 0),
                    500
                );
                set_ctrl_text(hwnd, BODY, "请备份今天的工作文件。");
                SendMessageW(GetDlgItem(hwnd, AUTO_CLOSE), BM_SETCHECK, 1, 0);
                SendMessageW(GetDlgItem(hwnd, SOUND_ENABLED), BM_SETCHECK, 1, 0);
                set_ctrl_text(hwnd, SOUND_FILE, "test-reminder.wav");
                SendMessageW(hwnd, WM_COMMAND, SOUND_PREVIEW as usize, 0);
                let state = &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State);
                assert!(state.preview.borrow().is_some());
                capture(hwnd, &shot);
                SendMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
            });
            let shot = shots.join("plan-editor-batch.png");
            queue(move |hwnd| {
                SendMessageW(
                    hwnd,
                    WM_COMMAND,
                    (RADIO
                        + Trigger::ALL
                            .iter()
                            .position(|trigger| *trigger == Trigger::Weekly)
                            .unwrap() as i32) as usize,
                    0,
                );
                SendMessageW(GetDlgItem(hwnd, ENABLED), BM_SETCHECK, 0, 0);
                capture(hwnd, &shot);
                SendMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
            });
            let shot = shots.join("plan-manager-saved.png");
            queue(move |hwnd| {
                let loaded = schedule::load(&snapshot().unwrap().1).unwrap();
                assert_eq!(loaded.tasks.len(), 1);
                assert_eq!(loaded.tasks[0].steps.len(), 4);
                assert_eq!(loaded.tasks[0].reminder, "请备份今天的工作文件。");
                assert!(loaded.tasks[0].reminder_options.allow_auto_close);
                assert!(loaded.tasks[0].reminder_options.play_sound);
                assert_eq!(
                    loaded.tasks[0].reminder_options.sound_file,
                    "test-reminder.wav"
                );
                assert!(!loaded.tasks[0].enabled);
                capture(hwnd, &shot);
                SendMessageW(hwnd, WM_COMMAND, MODIFY as usize, 0);
                arm(hwnd);
            });
            queue(|hwnd| {
                set_ctrl_text(hwnd, NAME, "取消后不得保存");
                SendMessageW(hwnd, WM_COMMAND, IDCANCEL as usize, 0);
            });
            queue(|hwnd| {
                assert_eq!(tasks()[0].name, "每日备份提醒");
                arm(hwnd);
                SendMessageW(hwnd, WM_COMMAND, DELETE as usize, 0);
                arm(hwnd);
            });
            queue(|hwnd| {
                let confirm = GetLastActivePopup(hwnd);
                assert_ne!(confirm, hwnd);
                SendMessageW(confirm, WM_COMMAND, IDYES as usize, 0);
            });
            queue(|hwnd| {
                assert!(tasks().is_empty());
                SendMessageW(hwnd, WM_COMMAND, IDCANCEL as usize, 0);
            });
            open(None);
        }
        unsafe {
            crate::popup::register(GetModuleHandleW(ptr::null()));
            let source = base.join("assets/icons/menu_schedule.png");
            let path = source.to_string_lossy().into_owned();
            queue(move |hwnd| {
                set_ctrl_text(hwnd, LINK, &path);
                arm(hwnd);
                SendMessageW(hwnd, WM_COMMAND, CONVERT as usize, 0);
                arm(hwnd);
            });
            let shot = shots.join("plan-convert-menu.png");
            queue(move |_| {
                let menu = crate::dialog_tests::menu_window();
                assert_ne!(menu, 0);
                capture(menu, &shot);
                PostMessageW(menu, WM_KEYDOWN, 40, 0);
                PostMessageW(menu, WM_KEYDOWN, 13, 0);
            });
            queue(|hwnd| {
                assert!(get_ctrl_text(hwnd, LINK).starts_with(".\\"));
                arm(hwnd);
                SendMessageW(hwnd, WM_COMMAND, CONVERT as usize, 0);
                arm(hwnd);
            });
            queue(|_| {
                let menu = crate::dialog_tests::menu_window();
                assert_ne!(menu, 0);
                PostMessageW(menu, WM_KEYDOWN, 40, 0);
                PostMessageW(menu, WM_KEYDOWN, 40, 0);
                PostMessageW(menu, WM_KEYDOWN, 13, 0);
            });
            queue(|hwnd| {
                assert!(std::path::Path::new(&get_ctrl_text(hwnd, LINK)).is_absolute());
                SendMessageW(hwnd, WM_COMMAND, IDCANCEL as usize, 0);
            });
            let mut task = Task::new(crate::local_clock::now(), "convert-test");
            task.name = "路径转换回归".into();
            show(
                0,
                0,
                base.clone(),
                "设置计划任务",
                (476, 416),
                Dialog::Editor { task, batch: false },
            );
            queue(|hwnd| {
                assert!(!get_ctrl_text(hwnd, NAME).is_empty());
                assert!(!get_ctrl_text(hwnd, LINK).is_empty());
                SendMessageW(hwnd, WM_COMMAND, IDCANCEL as usize, 0);
            });
            open(Some("sc_1"));
        }
        unsafe {
            let shot = shots.join("plan-reminder-notice.png");
            queue(move |hwnd| {
                assert!(crate::schedule_reminder::has_active());
                SendMessageW(hwnd, WM_TIMER, 53, 0);
                assert_ne!(IsWindow(hwnd), 0, "未允许自动关闭时必须保留提醒");
                capture(hwnd, &shot);
                SendMessageW(GetDlgItem(hwnd, IDOK), WM_KEYDOWN, 13, 0);
            });
            let notice = crate::schedule_reminder::show(
                0,
                &base,
                "计划任务提醒",
                "这是一条不会自动关闭的提醒。",
                ReminderOptions::default(),
            )
            .unwrap();
            pump_until_closed(notice);
            assert!(!crate::schedule_reminder::has_active());
            let shot = shots.join("plan-reminder-auto.png");
            queue(move |hwnd| {
                assert!(get_ctrl_text(hwnd, 1756).contains("60 秒"));
                capture(hwnd, &shot);
                crate::schedule_reminder::expire_for_test(hwnd);
            });
            let notice = crate::schedule_reminder::show(
                0,
                &base,
                "计划任务提醒",
                "60 秒后自动关闭，并播放指定声音。",
                ReminderOptions {
                    allow_auto_close: true,
                    play_sound: true,
                    sound_file: "test-reminder.wav".into(),
                },
            )
            .unwrap();
            pump_until_closed(notice);
            assert!(!crate::schedule_reminder::has_active());
            // 取消提醒编辑时，内容和声音选项均保持草稿原值。
            queue(|hwnd| {
                set_ctrl_text(hwnd, BODY, "不应保存");
                SendMessageW(GetDlgItem(hwnd, AUTO_CLOSE), BM_SETCHECK, 0, 0);
                SendMessageW(hwnd, WM_COMMAND, IDCANCEL as usize, 0);
            });
            assert!(show(
                0,
                0,
                base.clone(),
                "增加提醒",
                (398, 334),
                Dialog::Reminder {
                    text: "原提醒".into(),
                    options: ReminderOptions {
                        allow_auto_close: true,
                        ..Default::default()
                    }
                }
            )
            .is_none());
        }
        enable(false);
        assert!(schedule::load(&base).unwrap().tasks.is_empty());
        assert_eq!(
            std::fs::read(base.join("user/data.json")).unwrap(),
            original_data,
            "计划任务不得写入快捷方式数据"
        );
        // 用仅写入隔离目录的命令，验证真实执行及毫秒延时的先后顺序。
        let log = base.join("batch-result.txt");
        if log.exists() {
            std::fs::remove_file(&log).unwrap();
        }
        let mut task = Task::new(crate::local_clock::now(), "test");
        task.name = "批处理回归".into();
        task.steps = vec![
            Step::Launch {
                command: Command {
                    link: "cmd.exe".into(),
                    args: format!("/d /c echo first>\"{}\"", log.display()),
                    work_dir: String::new(),
                },
            },
            Step::Delay { milliseconds: 200 },
            Step::Launch {
                command: Command {
                    link: "cmd.exe".into(),
                    args: format!("/d /c echo second>>\"{}\"", log.display()),
                    work_dir: String::new(),
                },
            },
        ];
        crate::schedule_runtime::test(0, task);
        let start = std::time::Instant::now();
        loop {
            crate::schedule_runtime::tick(0);
            let text = std::fs::read_to_string(&log).unwrap_or_default();
            if text.contains("second") {
                assert!(text.find("first").unwrap() < text.find("second").unwrap());
                assert!(start.elapsed() >= std::time::Duration::from_millis(200));
                break;
            }
            assert!(
                start.elapsed() < std::time::Duration::from_secs(5),
                "计划任务批处理未完成"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let now = crate::local_clock::now();
        let mut countdown = Task::new(now, "test");
        countdown.name = "倒计时执行回归".into();
        countdown.trigger = Trigger::Countdown;
        countdown.seconds = 1;
        countdown.steps = vec![Step::Launch {
            command: Command {
                link: "cmd.exe".into(),
                args: format!("/d /c echo countdown>>\"{}\"", log.display()),
                work_dir: String::new(),
            },
        }];
        countdown.arm(now, "test");
        APP_STATE.with(|cell| {
            let mut guard = cell.try_borrow_mut().unwrap();
            let app = guard.as_mut().unwrap();
            app.plan = schedule::Plan {
                tasks: vec![countdown],
            };
            schedule::save(&base, &app.plan).unwrap();
        });
        let wait = std::time::Instant::now();
        loop {
            crate::schedule_runtime::tick(0);
            if std::fs::read_to_string(&log)
                .unwrap_or_default()
                .contains("countdown")
            {
                break;
            }
            assert!(
                wait.elapsed() < std::time::Duration::from_secs(5),
                "倒计时未触发"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let fired = schedule::load(&base).unwrap();
        assert!(!fired.tasks[0].enabled);
        assert!(fired.tasks[0].last_run.is_some());
        assert_eq!(
            std::fs::read(base.join("user/data.json")).unwrap(),
            original_data
        );
        let mut pending = Task::new(crate::local_clock::now(), "test");
        pending.name = "退出时取消旧批处理".into();
        pending.steps = vec![
            Step::Delay {
                milliseconds: 60_000,
            },
            Step::Launch {
                command: Command {
                    link: "cmd.exe".into(),
                    args: format!("/d /c echo unexpected>>\"{}\"", log.display()),
                    work_dir: String::new(),
                },
            },
        ];
        crate::schedule_runtime::test(0, pending);
        let mut exit = Task::new(crate::local_clock::now(), "test");
        exit.name = "退出计划回归".into();
        exit.trigger = Trigger::Exit;
        exit.steps = vec![Step::Launch {
            command: Command {
                link: "cmd.exe".into(),
                args: format!("/d /c echo exit-plan>>\"{}\"", log.display()),
                work_dir: String::new(),
            },
        }];
        APP_STATE.with(|cell| {
            let mut guard = cell.try_borrow_mut().unwrap();
            let app = guard.as_mut().unwrap();
            app.plan = schedule::Plan { tasks: vec![exit] };
            schedule::save(&base, &app.plan).unwrap();
        });
        crate::schedule_runtime::begin_exit(0);
        let wait = std::time::Instant::now();
        loop {
            let text = std::fs::read_to_string(&log).unwrap_or_default();
            if text.contains("exit-plan") {
                assert!(!text.contains("unexpected"));
                break;
            }
            assert!(
                wait.elapsed() < std::time::Duration::from_secs(3),
                "退出计划不应等待旧任务的延时"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(schedule::load(&base).unwrap().tasks[0].last_run.is_some());
        schedule::save(&base, &Default::default()).unwrap();
        APP_STATE.with(|cell| {
            cell.try_borrow_mut().unwrap().take();
        });
    }
}
