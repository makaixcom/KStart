//! 仅含网址、名称和图标的超链接添加窗口。
use crate::{
    native_dialogs::{blit_surface_on_bg, create_ctrl, get_ctrl_text, set_ctrl_text},
    skin_assets::RgbaSurface,
    to_wide,
    web_metadata::{self, Metadata},
};
use std::{cell::RefCell, mem, path::PathBuf, ptr, sync::mpsc};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::*,
        Input::KeyboardAndMouse::{EnableWindow, SetFocus},
        WindowsAndMessaging::*,
    },
};
const URL: i32 = 1600;
const NAME: i32 = 1601;
const ICON: i32 = 1602;
const PICK: i32 = 1603;
const STATUS: i32 = 1604;

pub struct Draft {
    pub url: String,
    pub name: String,
    pub icon: Option<RgbaSurface>,
    pub custom_icon: bool,
}
struct State {
    result: Option<Draft>,
    icon: Option<RgbaSurface>,
    fallback: Option<RgbaSurface>,
    name_manual: bool,
    icon_manual: bool,
    generation: u64,
    pending: bool,
    sender: mpsc::Sender<(u64, String)>,
    receiver: mpsc::Receiver<(u64, Metadata)>,
}

pub unsafe fn show(owner: HWND, base: PathBuf) -> Option<Draft> {
    let (send, receive) = mpsc::channel::<(u64, String)>();
    let (results, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(mut request) = receive.recv() {
            while let Ok(newer) = receive.try_recv() {
                request = newer;
            }
            let metadata = web_metadata::fetch(&request.1);
            if results.send((request.0, metadata)).is_err() {
                break;
            }
        }
    });
    let fallback = RgbaSurface::load_file(&base.join("icons/url.ico"));
    let state = RefCell::new(State {
        result: None,
        icon: fallback.clone(),
        fallback,
        name_manual: false,
        icon_manual: false,
        generation: 0,
        pending: false,
        sender: send,
        receiver,
    });
    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | DS_CENTER as u32;
    let mut template = vec![
        style as u16,
        (style >> 16) as u16,
        0,
        0,
        0,
        0,
        0,
        220,
        112,
        0,
        0,
    ];
    template.extend(to_wide("添加超链接"));
    let result = DialogBoxIndirectParamW(
        GetModuleHandleW(ptr::null()),
        template.as_ptr().cast(),
        owner,
        Some(proc),
        &state as *const _ as LPARAM,
    );
    if result == IDOK as isize {
        state.into_inner().result
    } else {
        None
    }
}

unsafe fn populate(hwnd: HWND) {
    let font = GetStockObject(DEFAULT_GUI_FONT) as HFONT;
    for (title, id, y) in [("网址(&U)：", URL, 18), ("名称(&N)：", NAME, 56)] {
        create_ctrl(hwnd, font, "STATIC", title, 0, 0, 14, y + 4, 70, 20, -1);
        create_ctrl(
            hwnd,
            font,
            "EDIT",
            "",
            WS_EX_CLIENTEDGE,
            WS_TABSTOP | ES_AUTOHSCROLL as u32,
            84,
            y,
            336,
            24,
            id,
        );
    }
    create_ctrl(hwnd, font, "STATIC", "图标：", 0, 0, 14, 100, 70, 20, -1);
    create_ctrl(
        hwnd,
        font,
        "BUTTON",
        "",
        0,
        WS_TABSTOP | BS_OWNERDRAW as u32,
        84,
        94,
        44,
        44,
        ICON,
    );
    create_ctrl(
        hwnd,
        font,
        "BUTTON",
        "选择图标…",
        0,
        WS_TABSTOP,
        142,
        103,
        100,
        26,
        PICK,
    );
    create_ctrl(hwnd, font, "STATIC", "", 0, 0, 14, 150, 406, 22, STATUS);
    create_ctrl(
        hwnd,
        font,
        "BUTTON",
        "确定",
        0,
        WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        254,
        182,
        78,
        26,
        IDOK,
    );
    create_ctrl(
        hwnd, font, "BUTTON", "取消", 0, WS_TABSTOP, 342, 182, 78, 26, IDCANCEL,
    );
    let mut rect = windows_sys::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: 434,
        bottom: 222,
    };
    AdjustWindowRectEx(&mut rect, GetWindowLongW(hwnd, GWL_STYLE) as u32, 0, 0);
    let mut owner: windows_sys::Win32::Foundation::RECT = mem::zeroed();
    GetWindowRect(GetParent(hwnd), &mut owner);
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    SetWindowPos(
        hwnd,
        0,
        owner.left + (owner.right - owner.left - w) / 2,
        owner.top + (owner.bottom - owner.top - h) / 2,
        w,
        h,
        SWP_NOZORDER,
    );
    crate::dialog_layout::install(hwnd, false);
    EnableWindow(GetDlgItem(hwnd, IDOK), 0);
    SetTimer(hwnd, 1, 100, None);
    SetFocus(GetDlgItem(hwnd, URL));
}

unsafe fn proc_state(hwnd: HWND) -> Option<&'static RefCell<State>> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<State>).as_ref()
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> isize {
    if msg == WM_INITDIALOG {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, lp);
        populate(hwnd);
        return 0;
    }
    if msg == WM_CLOSE || msg == WM_COMMAND && wp as u16 as i32 == IDCANCEL {
        EndDialog(hwnd, IDCANCEL as isize);
        return 1;
    }
    let Some(cell) = proc_state(hwnd) else {
        return 0;
    };
    if msg == WM_DRAWITEM {
        let draw = &*(lp as *const DRAWITEMSTRUCT);
        if draw.CtlID == ICON as u32 {
            FillRect(draw.hDC, &draw.rcItem, GetSysColorBrush(COLOR_BTNFACE));
            DrawEdge(draw.hDC, &mut draw.rcItem.clone(), BDR_RAISEDINNER, BF_RECT);
            if let Ok(state) = cell.try_borrow() {
                if let Some(icon) = &state.icon {
                    let color = GetSysColor(COLOR_BTNFACE).to_le_bytes();
                    blit_surface_on_bg(
                        draw.hDC,
                        icon,
                        draw.rcItem.left + (draw.rcItem.right - draw.rcItem.left - 32) / 2,
                        draw.rcItem.top + (draw.rcItem.bottom - draw.rcItem.top - 32) / 2,
                        32,
                        32,
                        [color[0], color[1], color[2]],
                    );
                }
            }
            return 1;
        }
    }
    if msg == WM_TIMER {
        let Ok(mut state) = cell.try_borrow_mut() else {
            return 0;
        };
        if wp == 2 {
            KillTimer(hwnd, 2);
            if let Some(url) = web_metadata::normalize(&get_ctrl_text(hwnd, URL)) {
                let generation = state.generation;
                let _ = state.sender.send((generation, url));
            }
        }
        while let Ok((generation, metadata)) = state.receiver.try_recv() {
            if generation != state.generation {
                continue;
            }
            state.pending = false;
            if !state.name_manual {
                let title = metadata.title.unwrap_or_else(|| {
                    web_metadata::normalize(&get_ctrl_text(hwnd, URL)).unwrap_or_default()
                });
                set_ctrl_text(hwnd, NAME, &title);
            }
            if !state.icon_manual {
                state.icon = metadata.icon.or_else(|| state.fallback.clone());
            }
            set_ctrl_text(hwnd, STATUS, "");
            EnableWindow(GetDlgItem(hwnd, IDOK), 1);
            InvalidateRect(GetDlgItem(hwnd, ICON), ptr::null(), 1);
        }
        return 1;
    }
    if msg != WM_COMMAND {
        return 0;
    }
    let id = wp as u16 as i32;
    let notification = (wp >> 16) as u16 as u32;
    if matches!(id, ICON | PICK) {
        // 不持有窗口状态借用进入原生文件选择消息循环。
        if let Some(path) = crate::pick_file_dialog(
            hwnd,
            "选择网址图标",
            &[
                ("图标和图片", "*.ico;*.png;*.bmp;*.jpg;*.gif"),
                ("所有文件", "*.*"),
            ],
        ) {
            if let Some(icon) = crate::load_custom_icon_16(&path) {
                if let Ok(mut state) = cell.try_borrow_mut() {
                    state.icon = Some(icon);
                    state.icon_manual = true;
                }
                InvalidateRect(GetDlgItem(hwnd, ICON), ptr::null(), 1);
            }
        }
        return 1;
    }
    let Ok(mut state) = cell.try_borrow_mut() else {
        return 0;
    };
    if id == URL && notification == EN_CHANGE {
        state.generation += 1;
        KillTimer(hwnd, 2);
        let valid = web_metadata::normalize(&get_ctrl_text(hwnd, URL)).is_some();
        state.pending = valid;
        if !state.name_manual {
            set_ctrl_text(hwnd, NAME, "");
        }
        if !state.icon_manual {
            state.icon = state.fallback.clone();
            InvalidateRect(GetDlgItem(hwnd, ICON), ptr::null(), 1);
        }
        EnableWindow(GetDlgItem(hwnd, IDOK), 0);
        set_ctrl_text(
            hwnd,
            STATUS,
            if valid {
                "正在获取网站名称和图标…"
            } else {
                ""
            },
        );
        if valid {
            SetTimer(hwnd, 2, 650, None);
        }
        return 1;
    }
    if id == NAME && notification == EN_CHANGE {
        state.name_manual = !get_ctrl_text(hwnd, NAME).trim().is_empty();
        return 1;
    }
    if id == IDOK && !state.pending {
        if let Some(url) = web_metadata::normalize(&get_ctrl_text(hwnd, URL)) {
            let name = get_ctrl_text(hwnd, NAME);
            let name = if name.trim().is_empty() {
                url.clone()
            } else {
                name.trim().into()
            };
            let icon = state.icon.clone();
            let custom_icon = state.icon_manual
                || match (&icon, &state.fallback) {
                    (Some(icon), Some(fallback)) => icon.pixels() != fallback.pixels(),
                    (Some(_), None) => true,
                    _ => false,
                };
            state.result = Some(Draft {
                url,
                name,
                icon,
                custom_icon,
            });
            drop(state);
            EndDialog(hwnd, IDOK as isize);
        }
        return 1;
    }
    0
}
