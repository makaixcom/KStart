//! 计划提醒的独立通知窗口及声音播放；提醒不阻塞其他计划任务。
use crate::{
    native_dialogs::{create_ctrl, set_ctrl_text},
    schedule::ReminderOptions,
    to_wide,
};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    ptr,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    Media::Multimedia::{mciGetErrorStringW, mciSendStringW},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{GetFocus, SetFocus},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};
pub const AUTO_CLOSE_SECONDS: u64 = 60;
const TIMER: usize = 53;
const STATUS: i32 = 1756;
thread_local! {static ACTIVE:RefCell<Vec<HWND>>=const{RefCell::new(Vec::new())};}
pub fn handle_message(message: &MSG) -> bool {
    let windows = ACTIVE.with_borrow(Clone::clone);
    windows
        .into_iter()
        .any(|hwnd| unsafe { IsDialogMessageW(hwnd, message) != 0 })
}
pub fn has_active() -> bool {
    ACTIVE.with_borrow(|windows| !windows.is_empty())
}
/// 正常退出前关闭此前的提醒，随后再显示本次退出任务的提醒。
pub fn close_all() {
    let windows = ACTIVE.with_borrow(Clone::clone);
    for hwnd in windows {
        unsafe {
            if IsWindow(hwnd) != 0 {
                DestroyWindow(hwnd);
            }
        }
    }
}
/// 每个播放实例使用独立的 MCI 别名，关闭窗口时停止播放并释放设备。
pub struct Playback {
    alias: String,
}
impl Playback {
    pub fn start(base: &Path, file: &str) -> Result<Self, String> {
        let path = crate::targets::resolve_path(base, file.trim());
        let name = path.to_string_lossy();
        if !path.is_file() || name.contains(['"', '\0', '\r', '\n']) {
            return Err("声音文件不存在或路径无效。".into());
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let alias = format!(
            "kstart_reminder_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        mci(&format!("open \"{name}\" alias {alias}"))?;
        let player = Self { alias };
        mci(&format!("play {}", player.alias))?;
        Ok(player)
    }
}
impl Drop for Playback {
    fn drop(&mut self) {
        let _ = mci(&format!("stop {}", self.alias));
        let _ = mci(&format!("close {}", self.alias));
    }
}
fn mci(command: &str) -> Result<(), String> {
    unsafe {
        let result = mciSendStringW(to_wide(command).as_ptr(), ptr::null_mut(), 0, 0);
        if result == 0 {
            return Ok(());
        }
        let mut buffer = [0u16; 256];
        if mciGetErrorStringW(result, buffer.as_mut_ptr(), buffer.len() as u32) != 0 {
            let end = buffer
                .iter()
                .position(|ch| *ch == 0)
                .unwrap_or(buffer.len());
            Err(format!(
                "无法播放声音：{}",
                String::from_utf16_lossy(&buffer[..end])
            ))
        } else {
            Err(format!("无法播放声音（错误 {result}）。"))
        }
    }
}
struct Notice {
    main: HWND,
    name: String,
    text: String,
    base: std::path::PathBuf,
    options: ReminderOptions,
    font: HFONT,
    own_font: bool,
    deadline: Cell<Option<Instant>>,
    audio: RefCell<Option<Playback>>,
    initialized: Cell<bool>,
    native_released: Cell<bool>,
}
impl Drop for Notice {
    fn drop(&mut self) {
        if self.own_font {
            unsafe {
                DeleteObject(self.font);
            }
        }
    }
}
/// 使用无模式窗口显示，主窗体隐藏或其他提醒存在时仍可正常到达。
pub fn show(
    main: HWND,
    base: &Path,
    name: &str,
    text: &str,
    options: ReminderOptions,
) -> Result<HWND, String> {
    unsafe {
        options.validate(text)?;
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
        let state = Rc::new(Notice {
            main,
            name: name.into(),
            text: text.into(),
            base: base.to_owned(),
            options,
            font: if font != 0 {
                font
            } else {
                GetStockObject(DEFAULT_GUI_FONT) as HFONT
            },
            own_font: font != 0,
            deadline: Cell::new(None),
            audio: RefCell::new(None),
            initialized: Cell::new(false),
            native_released: Cell::new(false),
        });
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_CENTER as u32;
        let ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
        let mut template = vec![
            style as u16,
            (style >> 16) as u16,
            ex as u16,
            (ex >> 16) as u16,
            0,
            0,
            0,
            100,
            100,
            0,
            0,
        ];
        template.extend(to_wide(name));
        // 调用方保留一份强引用，兼容创建失败时同步销毁窗口的路径。
        let native = Rc::into_raw(state.clone());
        let hwnd = CreateDialogIndirectParamW(
            GetModuleHandleW(ptr::null()),
            template.as_ptr().cast(),
            0,
            Some(proc),
            native as LPARAM,
        );
        if hwnd == 0 {
            if !state.native_released.replace(true) {
                drop(Rc::from_raw(native));
            }
            return Err(format!(
                "无法创建提醒窗口：{}",
                std::io::Error::last_os_error()
            ));
        }
        ShowWindow(hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(hwnd);
        Ok(hwnd)
    }
}
fn seconds_left(deadline: Instant, now: Instant) -> u64 {
    let remaining = deadline.saturating_duration_since(now);
    remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0)
}
unsafe fn status(hwnd: HWND, state: &Notice) {
    if let Some(deadline) = state.deadline.get() {
        set_ctrl_text(
            hwnd,
            STATUS,
            &format!("{} 秒后自动关闭", seconds_left(deadline, Instant::now())),
        );
    }
}
// 原生模态管理窗口的嵌套消息循环中，也保留提醒窗口的键盘操作。
unsafe extern "system" fn notice_key(
    hwnd: HWND,
    message: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    if message == WM_KEYDOWN {
        let parent = GetParent(hwnd);
        match wp as u16 {
            13 | 27 => {
                DestroyWindow(parent);
                return 0;
            }
            9 => {
                let button = GetDlgItem(parent, IDOK);
                SetFocus(if GetFocus() == button {
                    GetDlgItem(parent, 1745)
                } else {
                    button
                });
                return 0;
            }
            _ => {}
        }
    }
    if message == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(notice_key), id);
    }
    DefSubclassProc(hwnd, message, wp, lp)
}

unsafe extern "system" fn proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> isize {
    if message == WM_INITDIALOG {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, lp);
    }
    let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Notice;
    let Some(state) = raw.as_ref() else {
        return 0;
    };
    #[cfg(test)]
    if message == WM_TIMER && crate::dialog_tests::step(hwnd, wp) {
        return 1;
    }
    match message {
        WM_INITDIALOG => {
            state.initialized.set(true);
            ACTIVE.with_borrow_mut(|windows| windows.push(hwnd));
            let mut area = RECT {
                left: 0,
                top: 0,
                right: 400,
                bottom: 250,
            };
            AdjustWindowRectEx(
                &mut area,
                GetWindowLongW(hwnd, GWL_STYLE) as u32,
                0,
                GetWindowLongW(hwnd, GWL_EXSTYLE) as u32,
            );
            let monitor = MonitorFromWindow(state.main, MONITOR_DEFAULTTONEAREST);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of_val(&info) as u32;
            GetMonitorInfoW(monitor, &mut info);
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                info.rcWork.left
                    + (info.rcWork.right - info.rcWork.left - (area.right - area.left)) / 2,
                info.rcWork.top
                    + (info.rcWork.bottom - info.rcWork.top - (area.bottom - area.top)) / 2,
                area.right - area.left,
                area.bottom - area.top,
                SWP_NOACTIVATE,
            );
            // 采用标准只读文本框，保留换行、复制与滚动。
            let body = create_ctrl(
                hwnd,
                state.font,
                "EDIT",
                &state.text,
                WS_EX_CLIENTEDGE,
                WS_TABSTOP
                    | WS_VSCROLL
                    | ES_MULTILINE as u32
                    | ES_AUTOVSCROLL as u32
                    | ES_READONLY as u32,
                16,
                16,
                368,
                158,
                1745,
            );
            create_ctrl(
                hwnd, state.font, "STATIC", "", 0, 0, 16, 188, 368, 18, STATUS,
            );
            create_ctrl(
                hwnd,
                state.font,
                "BUTTON",
                "关闭",
                0,
                WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
                310,
                216,
                74,
                23,
                IDOK,
            );
            SetWindowSubclass(body, Some(notice_key), 1, 0);
            SetWindowSubclass(GetDlgItem(hwnd, IDOK), Some(notice_key), 1, 0);
            if state.options.allow_auto_close {
                state.deadline.set(Some(
                    Instant::now() + Duration::from_secs(AUTO_CLOSE_SECONDS),
                ));
                SetTimer(hwnd, TIMER, 1000, None);
                status(hwnd, state);
            }
            if state.options.play_sound {
                match Playback::start(&state.base, &state.options.sound_file) {
                    Ok(audio) => {
                        state.audio.replace(Some(audio));
                    }
                    Err(error) => {
                        set_ctrl_text(hwnd, STATUS, &error);
                        eprintln!("计划提醒“{}”：{error}", state.name);
                    }
                }
            }
            #[cfg(test)]
            crate::dialog_tests::arm(hwnd);
            SetFocus(GetDlgItem(hwnd, IDOK));
            0
        }
        WM_TIMER if wp == TIMER => {
            if state
                .deadline
                .get()
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                DestroyWindow(hwnd);
            } else {
                status(hwnd, state);
            }
            1
        }
        WM_COMMAND if matches!(wp as u16 as i32, IDOK | IDCANCEL) => {
            DestroyWindow(hwnd);
            1
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            1
        }
        WM_CTLCOLORDLG | WM_CTLCOLORSTATIC => {
            SetBkColor(wp as HDC, GetSysColor(COLOR_BTNFACE));
            GetSysColorBrush(COLOR_BTNFACE)
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if state.initialized.replace(false) {
                ACTIVE.with_borrow_mut(|windows| windows.retain(|window| *window != hwnd));
            }
            if !state.native_released.replace(true) {
                drop(Rc::from_raw(raw));
            }
            0
        }
        _ => 0,
    }
}
#[cfg(test)]
pub(crate) unsafe fn expire_for_test(hwnd: HWND) {
    let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Notice;
    let state = raw.as_ref().unwrap();
    assert_eq!(AUTO_CLOSE_SECONDS, 60);
    assert!(state.deadline.get().is_some());
    state
        .deadline
        .set(Some(Instant::now() - Duration::from_secs(1)));
    SendMessageW(hwnd, WM_TIMER, TIMER, 0);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn countdown_rounds_up_and_stops_at_zero() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(60);
        assert_eq!(seconds_left(deadline, now), 60);
        assert_eq!(
            seconds_left(deadline, now + Duration::from_millis(59001)),
            1
        );
        assert_eq!(seconds_left(deadline, now + Duration::from_secs(60)), 0);
    }
}
