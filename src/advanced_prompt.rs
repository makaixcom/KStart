//! 高级设置使用的字体、间距和口令对话框。

use crate::{
    advanced::{AdvancedSettings, FontSetting},
    advanced_style,
    native_dialogs::{create_ctrl, get_ctrl_text, set_ctrl_text},
    to_wide,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io, path::Path, ptr};
use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, EnumFontFamiliesExW, GetDC, GetStockObject, InvalidateRect, ReleaseDC,
    DEFAULT_CHARSET, DEFAULT_GUI_FONT, HFONT, LOGFONTW, PAINTSTRUCT, TEXTMETRICW,
};
use windows_sys::Win32::Security::Cryptography::{
    BCryptCloseAlgorithmProvider, BCryptDeriveKeyPBKDF2, BCryptGenRandom,
    BCryptOpenAlgorithmProvider, BCRYPT_ALG_HANDLE_HMAC_FLAG, BCRYPT_SHA256_ALGORITHM,
    BCRYPT_USE_SYSTEM_PREFERRED_RNG,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DialogBoxIndirectParamW, EndDialog, GetClientRect, GetDlgItem, GetWindowLongPtrW, SendMessageW,
    SetWindowLongPtrW, BS_DEFPUSHBUTTON, CBN_SELCHANGE, CBS_DROPDOWNLIST, CB_ADDSTRING,
    CB_FINDSTRINGEXACT, CB_GETCURSEL, CB_SETCURSEL, DS_CENTER, ES_AUTOHSCROLL, ES_PASSWORD,
    GWLP_USERDATA, IDCANCEL, IDOK, LBN_SELCHANGE, LBS_NOTIFY, LB_ADDSTRING, LB_GETCURSEL,
    LB_SETCURSEL, WM_COMMAND, WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_INITDIALOG, WM_PAINT,
    WM_PRINTCLIENT, WS_CAPTION, WS_EX_CLIENTEDGE, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
    WS_VSCROLL,
};

enum Prompt {
    Password {
        change: bool,
        existing: bool,
        current: String,
        new: String,
    },
    Fonts([FontSetting; 4]),
    Spacing {
        original: [u32; 5],
        values: [u32; 5],
        mode: usize,
    },
}

const ID_CURRENT: i32 = 1_000;
const ID_NEW: i32 = 1_001;
const ID_REPEAT: i32 = 1_002;
const ID_MODE: i32 = 1_003;
const ID_PIXELS: i32 = 1_004;
const ID_BEFORE: i32 = 1_005;
const ID_DEFAULT: i32 = 1_006;
const ID_FONT_SLOT: i32 = 1_010;
const ID_FONT_FACE: i32 = 1_011;
const ID_FONT_SIZE: i32 = 1_012;

unsafe extern "system" fn add_font(
    font: *const LOGFONTW,
    _: *const TEXTMETRICW,
    _: u32,
    data: LPARAM,
) -> i32 {
    let names = &mut *(data as *mut BTreeSet<String>);
    let name = &(*font).lfFaceName;
    let end = name.iter().position(|&ch| ch == 0).unwrap_or(name.len());
    if end > 0 && name[0] != b'@' as u16 {
        names.insert(String::from_utf16_lossy(&name[..end]));
    }
    1
}

unsafe fn select_font(hwnd: HWND, selection: &FontSetting) {
    let face = GetDlgItem(hwnd, ID_FONT_FACE);
    let name = to_wide(&selection.face);
    let index = SendMessageW(
        face,
        CB_FINDSTRINGEXACT,
        usize::MAX,
        name.as_ptr() as LPARAM,
    );
    SendMessageW(face, CB_SETCURSEL, index as WPARAM, 0);
    SendMessageW(
        GetDlgItem(hwnd, ID_FONT_SIZE),
        CB_SETCURSEL,
        (selection.points.clamp(5, 48) - 5) as WPARAM,
        0,
    );
}

unsafe fn font_slot(hwnd: HWND) -> usize {
    SendMessageW(GetDlgItem(hwnd, ID_FONT_SLOT), LB_GETCURSEL, 0, 0).clamp(0, 3) as usize
}

unsafe fn show(owner: HWND, title: &str, (w, h): (u16, u16), prompt: &mut Prompt) -> bool {
    let style = WS_POPUP | WS_VISIBLE | WS_CAPTION | WS_SYSMENU | DS_CENTER as u32;
    // DLGTEMPLATE 的 32 位字段在缓冲区中按两个 WORD 编码，后接菜单、窗口类和标题。
    let mut template = vec![
        style as u16,
        (style >> 16) as u16,
        0,
        0,
        0,
        0,
        0,
        w,
        h,
        0,
        0,
    ];
    template.extend(to_wide(title));
    let instance: HINSTANCE = GetModuleHandleW(ptr::null());
    DialogBoxIndirectParamW(
        instance,
        template.as_ptr().cast(),
        owner,
        Some(wnd_proc),
        prompt as *mut _ as isize,
    ) == IDOK as isize
}

unsafe fn populate(hwnd: HWND, prompt: &Prompt) {
    let font = GetStockObject(DEFAULT_GUI_FONT) as HFONT;
    let (button_x, button_y) = match prompt {
        Prompt::Password {
            change, existing, ..
        } => {
            let mut y = 15;
            if *existing {
                create_ctrl(
                    hwnd,
                    font,
                    "STATIC",
                    if *change {
                        "当前口令"
                    } else {
                        "请输入口令"
                    },
                    0,
                    0,
                    14,
                    y + 4,
                    84,
                    20,
                    -1,
                );
                create_ctrl(
                    hwnd,
                    font,
                    "EDIT",
                    "",
                    WS_EX_CLIENTEDGE,
                    WS_TABSTOP | ES_PASSWORD as u32 | ES_AUTOHSCROLL as u32,
                    101,
                    y,
                    180,
                    24,
                    ID_CURRENT,
                );
                y += 34;
            }
            if *change {
                for (label, id) in [("新口令（可留空）", ID_NEW), ("确认新口令", ID_REPEAT)]
                {
                    create_ctrl(hwnd, font, "STATIC", label, 0, 0, 14, y + 4, 86, 20, -1);
                    create_ctrl(
                        hwnd,
                        font,
                        "EDIT",
                        "",
                        WS_EX_CLIENTEDGE,
                        WS_TABSTOP | ES_PASSWORD as u32 | ES_AUTOHSCROLL as u32,
                        101,
                        y,
                        180,
                        24,
                        id,
                    );
                    y += 34;
                }
            }
            (126, y + 4)
        }
        Prompt::Fonts(fonts) => {
            let slots = create_ctrl(
                hwnd,
                font,
                "LISTBOX",
                "",
                WS_EX_CLIENTEDGE,
                WS_TABSTOP | LBS_NOTIFY as u32,
                14,
                28,
                176,
                86,
                ID_FONT_SLOT,
            );
            for name in [
                "快捷方式字体",
                "栏目标题字体",
                "类别标签字体",
                "面板标签字体",
            ] {
                SendMessageW(slots, LB_ADDSTRING, 0, to_wide(name).as_ptr() as LPARAM);
            }
            SendMessageW(slots, LB_SETCURSEL, 0, 0);
            create_ctrl(hwnd, font, "STATIC", "字体", 0, 0, 207, 27, 80, 20, -1);
            create_ctrl(hwnd, font, "STATIC", "大小", 0, 0, 207, 76, 80, 20, -1);
            create_ctrl(hwnd, font, "STATIC", "预览", 0, 0, 14, 124, 50, 20, -1);
            let combo_style = WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32;
            let faces = create_ctrl(
                hwnd,
                font,
                "COMBOBOX",
                "",
                0,
                combo_style,
                207,
                46,
                166,
                180,
                ID_FONT_FACE,
            );
            let sizes = create_ctrl(
                hwnd,
                font,
                "COMBOBOX",
                "",
                0,
                combo_style,
                207,
                95,
                166,
                170,
                ID_FONT_SIZE,
            );
            let mut names: BTreeSet<String> = fonts.iter().map(|font| font.face.clone()).collect();
            let dc = GetDC(hwnd);
            if dc != 0 {
                let mut filter: LOGFONTW = std::mem::zeroed();
                filter.lfCharSet = DEFAULT_CHARSET;
                EnumFontFamiliesExW(
                    dc,
                    &filter,
                    Some(add_font),
                    &mut names as *mut _ as LPARAM,
                    0,
                );
                ReleaseDC(hwnd, dc);
            }
            for name in names {
                SendMessageW(faces, CB_ADDSTRING, 0, to_wide(&name).as_ptr() as LPARAM);
            }
            for size in 5..=48 {
                SendMessageW(
                    sizes,
                    CB_ADDSTRING,
                    0,
                    to_wide(&size.to_string()).as_ptr() as LPARAM,
                );
            }
            select_font(hwnd, &fonts[0]);
            (198, 187)
        }
        Prompt::Spacing { values, .. } => {
            create_ctrl(hwnd, font, "STATIC", "样式", 0, 0, 14, 16, 45, 20, -1);
            let combo = create_ctrl(
                hwnd,
                font,
                "COMBOBOX",
                "",
                0,
                WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
                14,
                37,
                166,
                170,
                ID_MODE,
            );
            for label in [
                "小图标－正常",
                "小图标－标题隐藏",
                "大图标－正常",
                "大图标－标题隐藏",
                "大图标－标题居右",
            ] {
                SendMessageW(combo, CB_ADDSTRING, 0, to_wide(label).as_ptr() as isize);
            }
            SendMessageW(combo, CB_SETCURSEL, 0, 0);
            create_ctrl(
                hwnd,
                font,
                "STATIC",
                "间距（像素）",
                0,
                0,
                14,
                77,
                96,
                20,
                -1,
            );
            create_ctrl(
                hwnd,
                font,
                "EDIT",
                &values[0].to_string(),
                WS_EX_CLIENTEDGE,
                WS_TABSTOP,
                14,
                100,
                90,
                24,
                ID_PIXELS,
            );
            create_ctrl(
                hwnd,
                font,
                "BUTTON",
                "修改前",
                0,
                WS_TABSTOP,
                14,
                139,
                79,
                25,
                ID_BEFORE,
            );
            create_ctrl(
                hwnd,
                font,
                "BUTTON",
                "默认设置",
                0,
                WS_TABSTOP,
                101,
                139,
                79,
                25,
                ID_DEFAULT,
            );
            create_ctrl(hwnd, font, "STATIC", "预览", 0, 0, 200, 16, 165, 20, -1);
            (240, 208)
        }
    };
    let confirm = create_ctrl(
        hwnd,
        font,
        "BUTTON",
        "确定",
        0,
        WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        button_x,
        button_y,
        73,
        24,
        IDOK,
    );
    let cancel = create_ctrl(
        hwnd,
        font,
        "BUTTON",
        "取消",
        0,
        WS_TABSTOP,
        button_x + 81,
        button_y,
        73,
        24,
        IDCANCEL,
    );
    if visual_footer(prompt).is_some() {
        advanced_style::button(confirm, false, true);
        advanced_style::button(cancel, false, false);
        if matches!(prompt, Prompt::Spacing { .. }) {
            advanced_style::button(GetDlgItem(hwnd, ID_BEFORE), false, false);
            advanced_style::button(GetDlgItem(hwnd, ID_DEFAULT), false, false);
        }
    }
}

fn visual_footer(prompt: &Prompt) -> Option<i32> {
    match prompt {
        Prompt::Fonts(_) => Some(172),
        Prompt::Spacing { .. } => Some(197),
        _ => None,
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    if message == WM_INITDIALOG {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam);
        if let Some(state) = (lparam as *const Prompt).as_ref() {
            populate(hwnd, state);
        }
        return 1;
    }
    if message != WM_COMMAND {
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Prompt;
        if let Some((state, footer)) = state
            .as_ref()
            .and_then(|state| visual_footer(state).map(|footer| (state, footer)))
        {
            match message {
                WM_ERASEBKGND => return 1,
                WM_CTLCOLORSTATIC => {
                    return advanced_style::static_color(wparam as _) as isize;
                }
                WM_PAINT | WM_PRINTCLIENT => {
                    let mut paint: PAINTSTRUCT = std::mem::zeroed();
                    let mut client: RECT = std::mem::zeroed();
                    GetClientRect(hwnd, &mut client);
                    let dc = if message == WM_PAINT {
                        BeginPaint(hwnd, &mut paint)
                    } else {
                        wparam as _
                    };
                    advanced_style::paint_child(dc, client.right, client.bottom, footer);
                    match state {
                        Prompt::Fonts(fonts) => {
                            advanced_style::paint_font_preview(dc, &fonts[font_slot(hwnd)])
                        }
                        Prompt::Spacing { values, mode, .. } => {
                            advanced_style::paint_spacing_preview(dc, values[*mode], *mode)
                        }
                        _ => {}
                    }
                    if message == WM_PAINT {
                        EndPaint(hwnd, &paint);
                    }
                    return 1;
                }
                _ => {}
            }
        }
        return 0;
    }
    let id = (wparam & 0xffff) as i32;
    let notification = (wparam >> 16) as u32;
    if id == IDCANCEL {
        EndDialog(hwnd, IDCANCEL as isize);
        return 1;
    }
    if !matches!(
        id,
        IDOK | ID_MODE
            | ID_PIXELS
            | ID_BEFORE
            | ID_DEFAULT
            | ID_FONT_SLOT
            | ID_FONT_FACE
            | ID_FONT_SIZE
    ) {
        return 0;
    }
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Prompt;
    let Some(state) = state_ptr.as_mut() else {
        return 0;
    };
    match state {
        Prompt::Password {
            change,
            current,
            new,
            ..
        } if id == IDOK => {
            if *change {
                let candidate = get_ctrl_text(hwnd, ID_NEW);
                if candidate != get_ctrl_text(hwnd, ID_REPEAT) {
                    crate::advanced_runtime::message(hwnd, "两次输入的口令不一致。");
                    return 1;
                }
                *new = candidate;
            }
            *current = get_ctrl_text(hwnd, ID_CURRENT);
        }
        Prompt::Fonts(fonts) => {
            let selection = &mut fonts[font_slot(hwnd)];
            if id == ID_FONT_SLOT && notification == LBN_SELCHANGE {
                select_font(hwnd, selection);
            } else if id == ID_FONT_FACE && notification == CBN_SELCHANGE {
                selection.face = get_ctrl_text(hwnd, ID_FONT_FACE);
            } else if id == ID_FONT_SIZE && notification == CBN_SELCHANGE {
                if let Ok(points) = get_ctrl_text(hwnd, ID_FONT_SIZE).parse() {
                    selection.points = points;
                }
            }
            InvalidateRect(hwnd, ptr::null(), 0);
        }
        Prompt::Spacing {
            original,
            values,
            mode,
        } => {
            let entered = get_ctrl_text(hwnd, ID_PIXELS)
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|n| (50..=500).contains(n));
            if id == IDOK && entered.is_none() {
                crate::advanced_runtime::message(hwnd, "间距应在 50 到 500 像素之间。");
                return 1;
            }
            if let Some(value) = entered {
                values[*mode] = value;
            }
            if id == ID_MODE && notification == CBN_SELCHANGE {
                *mode =
                    (SendMessageW(GetDlgItem(hwnd, ID_MODE), CB_GETCURSEL, 0, 0) as usize).min(4);
            } else if id == ID_BEFORE {
                *values = *original;
            } else if id == ID_DEFAULT {
                *values = AdvancedSettings::default().spacing;
            }
            let value = values[*mode];
            if id == ID_PIXELS || id == ID_MODE || id == ID_BEFORE || id == ID_DEFAULT {
                InvalidateRect(hwnd, ptr::null(), 0);
            }
            if id == ID_MODE || id == ID_BEFORE || id == ID_DEFAULT {
                set_ctrl_text(hwnd, ID_PIXELS, &value.to_string());
            }
        }
        _ => {}
    }
    if id == IDOK {
        EndDialog(hwnd, IDOK as isize);
    }
    1
}

pub unsafe fn fonts(owner: HWND, fonts: &[FontSetting; 4]) -> Option<[FontSetting; 4]> {
    let mut prompt = Prompt::Fonts(fonts.clone());
    if !show(owner, "设置字体", (195, 115), &mut prompt) {
        return None;
    }
    if let Prompt::Fonts(values) = prompt {
        Some(values)
    } else {
        None
    }
}

pub unsafe fn spacing(owner: HWND, values: [u32; 5]) -> Option<[u32; 5]> {
    let mut prompt = Prompt::Spacing {
        original: values,
        values,
        mode: 0,
    };
    if !show(owner, "横排间距微调", (209, 126), &mut prompt) {
        return None;
    }
    if let Prompt::Spacing { values, .. } = prompt {
        Some(values)
    } else {
        None
    }
}

#[derive(Serialize, Deserialize)]
struct Credential {
    salt: [u8; 16],
    digest: [u8; 32],
}

fn digest(password: &str, salt: &[u8; 16]) -> io::Result<[u8; 32]> {
    let mut output = [0; 32];
    let mut alg = ptr::null_mut();
    unsafe {
        if BCryptOpenAlgorithmProvider(
            &mut alg,
            BCRYPT_SHA256_ALGORITHM,
            ptr::null(),
            BCRYPT_ALG_HANDLE_HMAC_FLAG,
        ) < 0
        {
            return Err(io::Error::other("无法初始化口令验证"));
        }
        let result = BCryptDeriveKeyPBKDF2(
            alg,
            password.as_ptr(),
            password.len() as u32,
            salt.as_ptr(),
            salt.len() as u32,
            600_000,
            output.as_mut_ptr(),
            output.len() as u32,
            0,
        );
        BCryptCloseAlgorithmProvider(alg, 0);
        if result < 0 {
            return Err(io::Error::other("无法验证口令"));
        }
    }
    Ok(output)
}

fn read_credential(base: &Path, lock: bool) -> io::Result<Option<Credential>> {
    let verification = crate::config::Verification::load(&base.join("user"));
    let value = if lock {
        &verification.lock
    } else {
        &verification.login
    };
    if value.is_empty() {
        Ok(None)
    } else {
        serde_json::from_str(value).map(Some).map_err(Into::into)
    }
}

fn matches_password(value: &str, expected: &Credential) -> io::Result<bool> {
    let actual = digest(value, &expected.salt)?;
    Ok(actual
        .iter()
        .zip(expected.digest)
        .fold(0u8, |acc, (&a, b)| acc | (a ^ b))
        == 0)
}

pub unsafe fn edit_password(owner: HWND, base: &Path, lock: bool) -> io::Result<()> {
    let stored = read_credential(base, lock)?;
    let mut prompt = Prompt::Password {
        change: true,
        existing: stored.is_some(),
        current: String::new(),
        new: String::new(),
    };
    if !show(
        owner,
        if lock {
            "设置锁定口令"
        } else {
            "设置登录口令"
        },
        (185, 103),
        &mut prompt,
    ) {
        return Ok(());
    }
    if let Prompt::Password { current, new, .. } = prompt {
        if let Some(stored) = &stored {
            if !matches_password(&current, stored)? {
                return Err(io::Error::other("当前口令不正确"));
            }
        }
        let value = if new.is_empty() {
            String::new()
        } else {
            let mut salt = [0; 16];
            if BCryptGenRandom(
                ptr::null_mut(),
                salt.as_mut_ptr(),
                salt.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            ) < 0
            {
                return Err(io::Error::other("无法生成口令盐值"));
            }
            serde_json::to_string(&Credential {
                salt,
                digest: digest(&new, &salt)?,
            })?
        };
        let user_dir = base.join("user");
        let mut verification = crate::config::Verification::load(&user_dir);
        if lock {
            verification.lock = value;
        } else {
            verification.login = value;
        }
        crate::config::UserConfig::load_from(&user_dir).save_to(
            &user_dir,
            &crate::config::HotkeyConfig::load_from(&user_dir),
            &verification,
        )?;
    }
    Ok(())
}

pub unsafe fn authenticate(owner: HWND, base: &Path, lock: bool) -> bool {
    let stored = match read_credential(base, lock) {
        Ok(Some(stored)) => stored,
        Ok(None) => return true,
        Err(error) => {
            crate::advanced_runtime::message(owner, &error.to_string());
            return false;
        }
    };
    loop {
        let mut prompt = Prompt::Password {
            change: false,
            existing: true,
            current: String::new(),
            new: String::new(),
        };
        if !show(
            owner,
            if lock {
                "解锁 KStart"
            } else {
                "登录验证"
            },
            (185, 62),
            &mut prompt,
        ) {
            return false;
        }
        if let Prompt::Password { current, .. } = prompt {
            match matches_password(&current, &stored) {
                Ok(true) => return true,
                Ok(false) => crate::advanced_runtime::message(owner, "口令不正确。"),
                Err(error) => {
                    crate::advanced_runtime::message(owner, &error.to_string());
                    return false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_digest_matches_only_the_original_password() {
        let salt = [17; 16];
        let credential = Credential {
            salt,
            digest: digest("回归口令", &salt).unwrap(),
        };
        assert!(matches_password("回归口令", &credential).unwrap());
        assert!(!matches_password("不是同一个口令", &credential).unwrap());
    }
}
