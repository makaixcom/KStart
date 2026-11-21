//! 热键记录、全局注册和输出文本。

use crate::{
    targets::{PathMode, TargetKind},
    APP_STATE,
};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, mem, ptr};
use windows_sys::Win32::{
    Foundation::{LocalFree, HWND},
    Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    },
    UI::{
        Input::KeyboardAndMouse::*,
        WindowsAndMessaging::{GetForegroundWindow, PostMessageW, WM_APP},
    },
};

pub const WM_SYNC: u32 = WM_APP + 18;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    pub enabled: bool,
    pub remark: String,
    #[serde(flatten)]
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    Shortcut {
        id: String,
    },
    Target {
        kind: TargetKind,
        target: String,
        path_mode: Option<PathMode>,
        args: String,
        work_dir: String,
        work_dir_mode: Option<PathMode>,
    },
    Text {
        encrypted: String,
    },
}

impl Binding {
    pub fn shortcut(id: &str) -> Self {
        Self {
            enabled: true,
            remark: String::new(),
            action: Action::Shortcut { id: id.into() },
        }
    }

    pub fn legacy(value: &str) -> Self {
        if value.starts_with("sc_") || value.is_empty() {
            return Self::shortcut(value);
        }
        let kind = TargetKind::infer(std::path::Path::new(""), value);
        let remark = match value {
            "/AH/" => "激活/隐藏音速启动",
            "/Search/" => "打开搜索窗口",
            _ => "",
        };
        Self {
            enabled: true,
            remark: remark.into(),
            action: Action::target(kind, value.into(), String::new(), String::new()),
        }
    }

    pub fn references(&self, id: &str) -> bool {
        matches!(&self.action, Action::Shortcut { id: current } if current == id)
    }
}

impl Action {
    pub fn target(kind: TargetKind, target: String, args: String, work_dir: String) -> Self {
        let path_mode = kind.path_mode(&target);
        let work_dir_mode = TargetKind::Folder.path_mode(&work_dir);
        Self::Target {
            kind,
            target,
            path_mode,
            args,
            work_dir,
            work_dir_mode,
        }
    }

    pub fn caption(&self) -> &str {
        match self {
            Self::Shortcut { id } => id,
            Self::Target { target, .. } => target,
            Self::Text { .. } => "输出文本",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub modifiers: u32,
    pub vk: u16,
}

const MODIFIERS: [(&str, u32); 4] = [
    ("Ctrl", MOD_CONTROL),
    ("Alt", MOD_ALT),
    ("Shift", MOD_SHIFT),
    ("Win", MOD_WIN),
];

const SPECIAL: &[(&str, u16)] = &[
    ("Space", VK_SPACE),
    ("Tab", VK_TAB),
    ("Enter", VK_RETURN),
    ("Esc", VK_ESCAPE),
    ("Backspace", VK_BACK),
    ("Delete", VK_DELETE),
    ("Insert", VK_INSERT),
    ("Up", VK_UP),
    ("Down", VK_DOWN),
    ("Left", VK_LEFT),
    ("Right", VK_RIGHT),
    ("Home", VK_HOME),
    ("End", VK_END),
    ("Page Up", VK_PRIOR),
    ("Page Down", VK_NEXT),
    ("`", VK_OEM_3),
    ("Minus", VK_OEM_MINUS),
    ("Equals", VK_OEM_PLUS),
    ("Comma", VK_OEM_COMMA),
    ("Period", VK_OEM_PERIOD),
    ("Slash", VK_OEM_2),
    ("Semicolon", VK_OEM_1),
    ("Quote", VK_OEM_7),
    ("Left Bracket", VK_OEM_4),
    ("Right Bracket", VK_OEM_6),
    ("Backslash", VK_OEM_5),
];

impl Key {
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split('+').map(str::trim).peekable();
        let mut modifiers = 0;
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                let name = part.to_ascii_uppercase();
                let vk = match name.as_bytes() {
                    [ch] if ch.is_ascii_alphanumeric() => *ch as u16,
                    _ => SPECIAL
                        .iter()
                        .find(|(label, _)| label.eq_ignore_ascii_case(part))
                        .map(|(_, vk)| *vk)
                        .or_else(|| {
                            name.strip_prefix('F')?
                                .parse::<u16>()
                                .ok()
                                .filter(|n| (1..=24).contains(n))
                                .map(|n| VK_F1 + n - 1)
                        })?,
                };
                return Some(Self { modifiers, vk });
            }
            let bits = MODIFIERS
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(part))?
                .1;
            if modifiers & bits != 0 {
                return None;
            }
            modifiers |= bits;
        }
        None
    }

    pub fn name(self) -> Option<String> {
        let mut parts: Vec<String> = MODIFIERS
            .iter()
            .filter(|(_, bits)| self.modifiers & bits != 0)
            .map(|(name, _)| (*name).into())
            .collect();
        let vk = self.vk;
        let suffix = if matches!(vk, 0x30..=0x39 | 0x41..=0x5a) {
            char::from(vk as u8).to_string()
        } else if (VK_F1..=VK_F24).contains(&vk) {
            format!("F{}", vk - VK_F1 + 1)
        } else {
            SPECIAL.iter().find(|(_, key)| *key == vk)?.0.into()
        };
        parts.push(suffix);
        Some(parts.join("+"))
    }
}

thread_local! {
    static REGISTERED: RefCell<Vec<(i32, Key)>> = const { RefCell::new(Vec::new()) };
}

pub unsafe fn sync(hwnd: HWND) {
    let keys = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        Some(
            app.hotkey_cfg
                .bindings
                .iter()
                .filter(|(_, binding)| app.user_cfg.enable_hotkey && binding.enabled)
                .filter_map(|(name, _)| Key::parse(name))
                .collect::<Vec<_>>(),
        )
    });
    let Some(keys) = keys else {
        return;
    };
    REGISTERED.with_borrow_mut(|registered| {
        registered.retain(|(id, key)| {
            if keys.contains(key) {
                true
            } else {
                UnregisterHotKey(hwnd, *id);
                false
            }
        });
        for key in keys {
            if registered.iter().any(|(_, current)| *current == key) {
                continue;
            }
            if let Some(id) = (1..=0xbfff).find(|id| !registered.iter().any(|(used, _)| used == id))
            {
                if RegisterHotKey(hwnd, id, key.modifiers | MOD_NOREPEAT, key.vk as u32) != 0 {
                    registered.push((id, key));
                } else {
                    eprintln!(
                        "注册热键 {} 失败：{}",
                        key.name().unwrap_or_default(),
                        std::io::Error::last_os_error()
                    );
                }
            }
        }
    });
}

pub fn is_registered(name: &str) -> bool {
    Key::parse(name).is_some_and(|key| {
        REGISTERED.with_borrow(|keys| keys.iter().any(|(_, candidate)| *candidate == key))
    })
}

pub unsafe fn clear(hwnd: HWND) {
    REGISTERED.with_borrow_mut(|keys| {
        for (id, _) in keys.drain(..) {
            UnregisterHotKey(hwnd, id);
        }
    });
}

pub fn changed(hwnd: HWND) {
    if hwnd != 0 {
        unsafe {
            PostMessageW(hwnd, WM_SYNC, 0, 0);
        }
    }
}

pub unsafe fn dispatch(hwnd: HWND, id: i32, reported: Key) {
    if IsWindowEnabled(hwnd) == 0 || !REGISTERED.with_borrow(|keys| keys.contains(&(id, reported)))
    {
        return;
    }
    let action = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        if !app.user_cfg.enable_hotkey || app.user_cfg.startup_lock || app.modal_hwnd != 0 {
            return None;
        }
        app.hotkey_cfg
            .bindings
            .iter()
            .find(|(name, binding)| binding.enabled && Key::parse(name) == Some(reported))
            .map(|(_, binding)| {
                (
                    binding.action.clone(),
                    app.base_dir.clone(),
                    app.user_cfg.advanced.clone(),
                )
            })
    });
    let Some((action, base, settings)) = action else {
        return;
    };
    match action {
        Action::Shortcut { id } => {
            let item = APP_STATE.with(|cell| {
                let guard = cell.try_borrow().ok()?;
                guard
                    .as_ref()?
                    .categories
                    .iter()
                    .flat_map(|cat| &cat.groups)
                    .flat_map(|group| &group.items)
                    .find(|item| item.id == id)
                    .cloned()
            });
            if let Some(item) = item {
                crate::launch_shortcut(&item, hwnd);
            }
        }
        Action::Target {
            kind,
            target,
            args,
            work_dir,
            ..
        } => {
            crate::launch_typed_target(kind, &target, &args, &work_dir, hwnd, &base, &settings);
        }
        Action::Text { encrypted } => {
            let foreground = GetForegroundWindow();
            if foreground != 0 && foreground != hwnd {
                if let Some(text) = decrypt(&encrypted) {
                    queue_text(hwnd, foreground, text, reported);
                }
            }
        }
    }
}

fn protect(bytes: &[u8], decrypt: bool) -> Option<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().ok()?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    unsafe {
        let ok = if decrypt {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 || output.pbData.is_null() {
            return None;
        }
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        Some(result)
    }
}

pub fn encrypt(text: &str) -> Option<String> {
    Some(
        protect(text.as_bytes(), false)?
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

pub fn decrypt(hex: &str) -> Option<String> {
    if !hex.is_ascii() || hex.len() % 2 != 0 {
        return None;
    }
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(protect(&bytes, true)?).ok()
}

pub const CONTROLS: &[(&str, &str, u16)] = &[
    ("回车", "Enter", VK_RETURN),
    ("空格", "Space", VK_SPACE),
    ("Tab", "Tab", VK_TAB),
    ("Esc", "Esc", VK_ESCAPE),
    ("Delete", "Delete", VK_DELETE),
    ("↑", "Up", VK_UP),
    ("↓", "Down", VK_DOWN),
    ("←", "Left", VK_LEFT),
    ("→", "Right", VK_RIGHT),
    ("Home", "Home", VK_HOME),
    ("End", "End", VK_END),
    ("Page Up", "Page Up", VK_PRIOR),
    ("Page Down", "Page Down", VK_NEXT),
];

struct PendingText {
    foreground: HWND,
    text: String,
    key: Key,
    expiry: std::time::Instant,
}

thread_local! {
    static PENDING_TEXT: RefCell<Option<PendingText>> = const { RefCell::new(None) };
}

pub const TEXT_TIMER: usize = 19;

unsafe fn queue_text(hwnd: HWND, foreground: HWND, text: String, key: Key) {
    PENDING_TEXT.set(Some(PendingText {
        foreground,
        text,
        key,
        expiry: std::time::Instant::now() + std::time::Duration::from_secs(3),
    }));
    if windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(hwnd, TEXT_TIMER, 20, None) == 0 {
        PENDING_TEXT.set(None);
    }
}

pub unsafe fn output_text(hwnd: HWND) {
    let permitted = IsWindowEnabled(hwnd) != 0
        && APP_STATE.with(|cell| {
            cell.try_borrow()
                .ok()
                .and_then(|guard| {
                    guard.as_ref().map(|app| {
                        app.user_cfg.enable_hotkey
                            && !app.user_cfg.startup_lock
                            && app.modal_hwnd == 0
                    })
                })
                .unwrap_or(false)
        });
    let ready = PENDING_TEXT.with_borrow(|pending| {
        pending.as_ref().map(|pending| {
            let down = |vk| GetAsyncKeyState(vk as i32) < 0;
            if !permitted
                || GetForegroundWindow() != pending.foreground
                || std::time::Instant::now() >= pending.expiry
            {
                None
            } else {
                Some(
                    ![
                        pending.key.vk,
                        VK_CONTROL,
                        VK_SHIFT,
                        VK_MENU,
                        VK_LWIN,
                        VK_RWIN,
                    ]
                    .into_iter()
                    .any(down),
                )
            }
        })
    });
    if ready == Some(Some(false)) {
        return;
    }
    windows_sys::Win32::UI::WindowsAndMessaging::KillTimer(hwnd, TEXT_TIMER);
    if let Some(pending) = PENDING_TEXT.take().filter(|_| ready == Some(Some(true))) {
        let events = text_events(&pending.text);
        if !events.is_empty() {
            SendInput(
                events.len() as u32,
                events.as_ptr(),
                mem::size_of::<INPUT>() as i32,
            );
        }
    }
}

fn text_events(mut text: &str) -> Vec<INPUT> {
    let pair = |vk, scan, flags| {
        [flags, flags | KEYEVENTF_KEYUP].map(|dw_flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: dw_flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        })
    };
    let mut events = Vec::new();
    while !text.is_empty() {
        if let Some(token) = text.strip_prefix('{').and_then(|rest| rest.split_once('}')) {
            if let Some((_, _, vk)) = CONTROLS.iter().find(|(_, name, _)| *name == token.0) {
                let extended = matches!(
                    *vk,
                    VK_DELETE
                        | VK_UP
                        | VK_DOWN
                        | VK_LEFT
                        | VK_RIGHT
                        | VK_HOME
                        | VK_END
                        | VK_PRIOR
                        | VK_NEXT
                );
                events.extend(pair(
                    *vk,
                    0,
                    if extended { KEYEVENTF_EXTENDEDKEY } else { 0 },
                ));
                text = token.1;
                continue;
            }
        }
        let ch = text.chars().next().unwrap();
        text = &text[ch.len_utf8()..];
        match ch {
            '\r' if text.starts_with('\n') => {}
            '\r' | '\n' => events.extend(pair(VK_RETURN, 0, 0)),
            '\t' => events.extend(pair(VK_TAB, 0, 0)),
            _ => {
                for unit in ch.encode_utf16(&mut [0; 2]) {
                    events.extend(pair(0, *unit, KEYEVENTF_UNICODE));
                }
            }
        }
    }
    events
}
