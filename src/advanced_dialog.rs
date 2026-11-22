//! 高级设置的原生控件及草稿读写。

use crate::advanced::{self, AdvancedSettings, FieldKind, Key};
use crate::native_dialogs::{create_ctrl, get_ctrl_text, set_ctrl_text};
use crate::to_wide;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{HDC, HFONT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetDlgItem, SendMessageW, ShowWindow, BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX,
    BS_DEFPUSHBUTTON, CBS_DROPDOWNLIST, CB_ADDSTRING, CB_GETCURSEL, CB_SETCURSEL, ES_AUTOHSCROLL,
    IDCANCEL, IDOK, LBN_SELCHANGE, LBS_HASSTRINGS, LBS_NOTIFY, LBS_OWNERDRAWFIXED, LB_ADDSTRING,
    LB_GETCURSEL, LB_SETCURSEL, LB_SETITEMHEIGHT, SW_HIDE, SW_SHOW, WS_EX_CLIENTEDGE, WS_TABSTOP,
    WS_VSCROLL, SBM_GETPOS, SBM_SETPOS, SBM_SETRANGE, SB_LINELEFT, SB_LINERIGHT,
    SB_PAGELEFT, SB_PAGERIGHT, SB_THUMBTRACK, SB_THUMBPOSITION, SB_LEFT, SB_RIGHT,
};

pub const WIDTH: i32 = 504;
pub const HEIGHT: i32 = 448;
pub const ID_PAGE: i32 = 2000;
pub const ID_RESET: i32 = 2001;
pub const ID_CLEAR_CACHE: i32 = 2002;
pub const ID_BACKUP: i32 = 2003;
pub const ID_RESTORE: i32 = 2004;
pub const ID_DESKTOP: i32 = 2005;
pub const ID_START_MENU: i32 = 2006;
pub const ID_LOGIN_PASSWORD: i32 = 2007;
pub const ID_LOCK_PASSWORD: i32 = 2008;
pub const ID_FONT: i32 = 2009;
pub const ID_SPACING: i32 = 2010;
const ID_OPACITY_VALUE: i32 = 2013;
const ID_FIELD: i32 = 2100;
const PAGE_STRIDE: i32 = 100;
const LABEL_OFFSET: i32 = 40;
const PICK_OFFSET: i32 = 70;

fn field_id(page: usize, index: usize) -> i32 {
    ID_FIELD + page as i32 * PAGE_STRIDE + index as i32
}

pub fn color_field(id:i32)->Option<Key> {
    let offset=id-ID_FIELD;if offset<0{return None;}
    let field=advanced::FIELDS.get((offset/PAGE_STRIDE) as usize)?.get((offset%PAGE_STRIDE) as usize)?;
    matches!(field.key,Key::ListBackgroundColor|Key::GroupColor|Key::DefaultColor).then_some(field.key)
}

fn has_picker(key: Key) -> bool {
    matches!(
        key,
        Key::FolderProgram
            | Key::CustomBrowser
            | Key::ListBackground
            | Key::ListBackgroundColor
            | Key::GroupColor
            | Key::DefaultColor
            | Key::TimeFormat
    )
}

pub fn picker(id: i32) -> Option<(i32, Key)> {
    let offset = id - ID_FIELD;
    if offset < 0 || offset % PAGE_STRIDE < PICK_OFFSET {
        return None;
    }
    let field = advanced::FIELDS
        .get((offset / PAGE_STRIDE) as usize)?
        .get((offset % PAGE_STRIDE - PICK_OFFSET) as usize)?;
    has_picker(field.key).then_some((id - PICK_OFFSET, field.key))
}

unsafe fn add_labels(ctrl: HWND, labels: impl IntoIterator<Item = impl AsRef<str>>) {
    for label in labels {
        let wide = to_wide(label.as_ref());
        SendMessageW(ctrl, CB_ADDSTRING, 0, wide.as_ptr() as isize);
    }
}

const FOOTER_Y: i32 = 409;

/// 各页沿用已有字段的顺序。第 4 项是在控件前为标签保留的宽度。
const POSITIONS: [&[(i32, i32, i32, i32)]; 6] = [
    &[
        (153, 41, 165, 0),
        (320, 41, 172, 0),
        (153, 64, 165, 0),
        (320, 64, 172, 0),
        (153, 87, 165, 0),
        (320, 87, 172, 0),
        (153, 110, 336, 0),
        (153, 133, 336, 0),
        (162, 159, 94, 0),
        (258, 159, 94, 0),
        (354, 159, 130, 0),
        (162, 181, 150, 0),
        (319, 181, 173, 0),
        (153, 204, 336, 0),
        (153, 255, 336, 117),
        (153, 279, 171, 0),
        (324, 279, 168, 0),
        (153, 339, 162, 109),
        (323, 339, 165, 111),
    ],
    &[
        (153, 40, 338, 0),
        (153, 62, 338, 0),
        (153, 84, 338, 0),
        (153, 106, 338, 0),
        (153, 128, 338, 0),
        (162, 152, 327, 146),
        (153, 175, 338, 0),
        (153, 201, 336, 119),
        (153, 256, 336, 126),
        (153, 313, 336, 99),
        (153, 340, 336, 126),
    ],
    &[
        (153, 39, 338, 0),
        (153, 60, 171, 0),
        (326, 60, 166, 0),
        (153, 81, 337, 0),
        (153, 124, 164, 0),
        (319, 124, 173, 0),
        (153, 146, 164, 0),
        (319, 146, 173, 0),
        (153, 205, 195, 0),
        (346, 205, 146, 0),
        (153, 227, 338, 0),
        (153, 272, 333, 180),
        (153, 316, 222, 0),
        (371, 316, 118, 82),
        (153, 370, 96, 0),
        (253, 370, 120, 0),
        (376, 370, 113, 0),
    ],
    &[
        (153, 22, 336, 88),
        (153, 71, 336, 107),
        (153, 96, 336, 107),
        (153, 121, 336, 107),
        (153, 158, 118, 0),
        (274, 158, 100, 0),
        (153, 183, 118, 0),
        (274, 183, 100, 0),
        (153, 208, 169, 0),
        (322, 208, 171, 0),
        (153, 235, 182, 69),
        (346, 235, 143, 0),
        (153, 263, 169, 0),
        (323, 263, 171, 0),
        (153, 293, 336, 126),
        (153, 320, 336, 126),
        (153, 347, 336, 126),
    ],
    &[(153, 35, 336, 0), (153, 70, 201, 0)],
    &[
        (366, 212, 126, 0),
        (153, 332, 336, 104),
        (153, 360, 336, 192),
    ],
];

const SECTIONS: &[(usize, &str, i32)] = &[
    (0, "程序相关", 15),
    (0, "显示托盘图标", 232),
    (0, "列表弹出与弹入", 314),
    (1, "运行相关", 15),
    (1, "文件夹打开", 232),
    (1, "网址打开", 288),
    (2, "快捷方式", 15),
    (2, "快捷方式图标", 101),
    (2, "执行系统功能", 183),
    (2, "光驱", 248),
    (2, "音量控制", 295),
    (2, "清空回收站", 345),
    (3, "颜色", 48),
    (5, "KStart 的快捷方式", 91),
    (5, "数据备份与恢复", 184),
    (5, "验证", 245),
    (5, "KStart 进程", 304),
];

const ACTIONS: &[(i32, usize, &str, i32, i32, i32)] = &[
    (ID_CLEAR_CACHE, 2, "清空缓存", 410, 166, 78),
    (ID_FONT, 3, "设置字体", 398, 182, 91),
    (ID_SPACING, 3, "间距调整", 398, 156, 91),
    (ID_BACKUP, 5, "数据备份", 153, 210, 95),
    (ID_RESTORE, 5, "数据恢复", 255, 210, 95),
    (ID_DESKTOP, 5, "添加到桌面", 153, 118, 106),
    (ID_START_MENU, 5, "添加到开始菜单", 269, 118, 132),
    (ID_LOGIN_PASSWORD, 5, "设置登录口令", 153, 269, 123),
    (ID_LOCK_PASSWORD, 5, "设置锁定口令", 286, 269, 123),
];

fn field_label(field: &advanced::Field) -> &str {
    match field.key {
        Key::RecycleNoConfirm => "不提示",
        Key::RecycleNoProgress => "不显示进度",
        Key::RecycleSilent => "没有声效",
        Key::AutoBackup => "自动备份",
        Key::FolderProgram => "第三方程序路径",
        Key::ListBackgroundColor => "列表背景色",
        Key::GroupColor => "栏目文字色",
        Key::DefaultColor => "默认文字色",
        Key::VolumeStep => "幅度（%）",
        Key::ListOpacity => "列表透明：",
        Key::ListBackground => "列表背景：",
        Key::TimeFormat => "时间格式：",
        Key::MemoryLimit => "工作集上限（MB，0 为默认）",
        _ => field.label,
    }
}

pub unsafe fn populate(
    hwnd: HWND,
    font: HFONT,
    settings: &AdvancedSettings,
    skins: &[String],
    selected_skin: usize,
    categories: &[(String, String)],
) {
    let list = create_ctrl(
        hwnd,
        font,
        "LISTBOX",
        "",
        0,
        WS_TABSTOP | (LBS_NOTIFY | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS) as u32,
        15,
        38,
        117,
        345,
        ID_PAGE,
    );
    for label in advanced::PAGES {
        SendMessageW(list, LB_ADDSTRING, 0, to_wide(label).as_ptr() as isize);
    }
    SendMessageW(list, LB_SETITEMHEIGHT, 0, 30);
    SendMessageW(list, LB_SETCURSEL, 0, 0);

    for (page, fields) in advanced::FIELDS.iter().enumerate() {
        assert_eq!(POSITIONS[page].len(), fields.len());
        for (index, field) in fields.iter().enumerate() {
            let id = field_id(page, index);
            let (x, y, width, label_width) = POSITIONS[page][index];
            let y=if page==5{y-74}else{y};
            let label = field_label(field);
            if matches!(field.kind, FieldKind::Category) {
                create_ctrl(
                    hwnd,
                    font,
                    "STATIC",
                    label,
                    0,
                    0,
                    x,
                    y,
                    width,
                    18,
                    id + LABEL_OFFSET,
                );
                let items = create_ctrl(
                    hwnd,
                    font,
                    "LISTBOX",
                    "",
                    WS_EX_CLIENTEDGE,
                    WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY as u32,
                    x,
                    y + 23,
                    width,
                    124,
                    id,
                );
                for (_, name) in categories {
                    SendMessageW(items, LB_ADDSTRING, 0, to_wide(name).as_ptr() as isize);
                }
                let selection = categories
                    .iter()
                    .position(|(id, _)| id == settings.text(Key::TrayCategory))
                    .unwrap_or(0);
                SendMessageW(items, LB_SETCURSEL, selection, 0);
            } else if matches!(field.kind, FieldKind::Check(_)) {
                let control = create_ctrl(
                    hwnd,
                    font,
                    "BUTTON",
                    label,
                    0,
                    WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                    x,
                    y,
                    width,
                    21,
                    id,
                );
                super::advanced_style::button(control, true, false);
            } else {
                create_ctrl(
                    hwnd,
                    font,
                    "STATIC",
                    label,
                    0,
                    0,
                    x,
                    y + 4,
                    label_width - 4,
                    17,
                    id + LABEL_OFFSET,
                );
                let field_x = x + label_width;
                let field_width = width - label_width;
                if matches!(field.kind, FieldKind::Choice(..) | FieldKind::Skin) {
                    let control = create_ctrl(
                        hwnd,
                        font,
                        "COMBOBOX",
                        "",
                        0,
                        WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
                        field_x,
                        y,
                        field_width,
                        240,
                        id,
                    );
                    match field.kind {
                        FieldKind::Choice(labels, _) => add_labels(control, labels),
                        FieldKind::Skin => {
                            add_labels(control, skins);
                            SendMessageW(control, CB_SETCURSEL, selected_skin, 0);
                        }
                        _ => {}
                    }
                } else if matches!(field.key,Key::ListBackgroundColor|Key::GroupColor|Key::DefaultColor) {
                    create_ctrl(hwnd,font,"BUTTON","",0,WS_TABSTOP|windows_sys::Win32::UI::WindowsAndMessaging::BS_OWNERDRAW as u32,
                        field_x,y,field_width,22,id);
                } else if field.key == Key::ListOpacity {
                    let slider = create_ctrl(hwnd, font, "SCROLLBAR", "", 0,
                        WS_TABSTOP, field_x, y + 3, field_width - 64, 18, id);
                    SendMessageW(slider, SBM_SETRANGE, 10, 100);
                    create_ctrl(hwnd, font, "STATIC", "", 0, 0,
                        field_x + field_width - 60, y + 4, 60, 17, ID_OPACITY_VALUE);
                } else {
                    let picker = has_picker(field.key);
                    create_ctrl(
                        hwnd,
                        font,
                        "EDIT",
                        "",
                        WS_EX_CLIENTEDGE,
                        WS_TABSTOP | ES_AUTOHSCROLL as u32,
                        field_x,
                        y,
                        field_width - if picker { 31 } else { 0 },
                        22,
                        id,
                    );
                    if picker {
                        let icon=matches!(field.key,Key::ListBackground|Key::TimeFormat);
                        let control = create_ctrl(
                            hwnd,
                            font,
                            "BUTTON",
                            if icon { "" } else { "…" },
                            0,
                            WS_TABSTOP | if icon { windows_sys::Win32::UI::WindowsAndMessaging::BS_OWNERDRAW as u32 } else { 0 },
                            x + width - 26,
                            y,
                            26,
                            22,
                            id + PICK_OFFSET,
                        );
                        if icon { install_icon_tip(hwnd,control,field.key); }
                        else {super::advanced_style::button(control, false, false);}
                    }
                }
            }
        }
    }
    for &(id, page, label, x, y, width) in ACTIONS {
        let y=if page==5{y-74}else{y};
        let control = create_ctrl(
            hwnd,
            font,
            "BUTTON",
            label,
            0,
            WS_TABSTOP,
            x,
            y,
            width,
            23,
            id,
        );
        super::advanced_style::button(control, false, false);
    }
    for (id, label, x) in [
        (IDOK, "确定", 258),
        (IDCANCEL, "取消", 336),
        (ID_RESET, "重置", 414),
    ] {
        let control = create_ctrl(
            hwnd,
            font,
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
            FOOTER_Y + 10,
            73,
            23,
            id,
        );
        super::advanced_style::button(control, false, id == IDOK);
    }
    for page in 0..advanced::PAGES.len() {
        load_page(hwnd, page, settings);
    }
    show_page(hwnd);
}

pub unsafe fn paint(hwnd: HWND, hdc: HDC, font: HFONT) {
    let page = current_page(hwnd);
    super::advanced_style::paint_panel(hdc, font, WIDTH, HEIGHT, FOOTER_Y, section_rows(page));
}

fn section_rows(page: usize) -> impl Iterator<Item = (&'static str, i32)> {
    SECTIONS
        .iter()
        .filter(move |&&(section, _, _)| section == page)
        .map(move |&(_, name, y)| (name, if page==5{y-74}else{y}))
}

unsafe fn load_page(hwnd: HWND, page: usize, settings: &AdvancedSettings) {
    for (index, field) in advanced::FIELDS[page].iter().enumerate() {
        let id = field_id(page, index);
        let ctrl = GetDlgItem(hwnd, id);
        if field.key == Key::ListOpacity {
            let value = settings.number(field.key);
            SendMessageW(ctrl, SBM_SETPOS, value as usize, 1);
            set_ctrl_text(hwnd, ID_OPACITY_VALUE, &opacity_status(value as isize));
            continue;
        }
        match field.kind {
            FieldKind::Check(_) => {
                SendMessageW(
                    ctrl,
                    BM_SETCHECK,
                    usize::from(settings.enabled(field.key)),
                    0,
                );
            }
            FieldKind::Choice(..) => {
                SendMessageW(ctrl, CB_SETCURSEL, settings.number(field.key) as usize, 0);
            }
            FieldKind::Text(_) => set_ctrl_text(hwnd, id, settings.text(field.key)),
            FieldKind::Number(..) => {
                set_ctrl_text(hwnd, id, &settings.number(field.key).to_string())
            }
            _ => {}
        }
    }
}

unsafe fn current_page(hwnd: HWND) -> usize {
    (SendMessageW(GetDlgItem(hwnd, ID_PAGE), LB_GETCURSEL, 0, 0) as usize)
        .min(advanced::PAGES.len() - 1)
}

pub unsafe fn show_page(hwnd: HWND) {
    let selected = current_page(hwnd);
    ShowWindow(GetDlgItem(hwnd, ID_OPACITY_VALUE), if selected == 3 { SW_SHOW } else { SW_HIDE });
    for (page, fields) in advanced::FIELDS.iter().enumerate() {
        let show = if selected == page { SW_SHOW } else { SW_HIDE };
        for (index, _) in fields.iter().enumerate() {
            let id = field_id(page, index);
            for offset in [0, LABEL_OFFSET, PICK_OFFSET] {
                let ctrl = GetDlgItem(hwnd, id + offset);
                if ctrl != 0 {
                    ShowWindow(ctrl, show);
                }
            }
        }
    }
    for &(id, page, ..) in ACTIONS {
        ShowWindow(
            GetDlgItem(hwnd, id),
            if selected == page { SW_SHOW } else { SW_HIDE },
        );
    }
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, std::ptr::null(), 1);
}

pub unsafe fn select_page(hwnd: HWND, page: usize) {
    SendMessageW(GetDlgItem(hwnd, ID_PAGE), LB_SETCURSEL, page.min(advanced::PAGES.len() - 1), 0);
    show_page(hwnd);
}

pub unsafe fn reset_page(hwnd: HWND) -> usize {
    let page = current_page(hwnd);
    load_page(hwnd, page, &AdvancedSettings::default());
    if page == 3 {
        SendMessageW(GetDlgItem(hwnd,field_id(3,0)),CB_SETCURSEL,0,0);
    }
    if page == 4 {
        SendMessageW(GetDlgItem(hwnd, field_id(4, 1)), LB_SETCURSEL, 0, 0);
    }
    page
}

pub unsafe fn read(
    hwnd: HWND,
    mut draft: AdvancedSettings,
    categories: &[(String, String)],
) -> Result<(AdvancedSettings, usize), String> {
    let mut skin = 0;
    for (page, fields) in advanced::FIELDS.iter().enumerate() {
        for (index, field) in fields.iter().enumerate() {
            let id = field_id(page, index);
            let ctrl = GetDlgItem(hwnd, id);
            if field.key == Key::ListOpacity {
                draft.set(field.key, SendMessageW(ctrl, SBM_GETPOS, 0, 0).clamp(10, 100).to_string());
                continue;
            }
            let value = match field.kind {
                FieldKind::Check(_) => (SendMessageW(ctrl, BM_GETCHECK, 0, 0) != 0)
                    .then_some("1")
                    .unwrap_or("0")
                    .into(),
                FieldKind::Choice(..) => SendMessageW(ctrl, CB_GETCURSEL, 0, 0).max(0).to_string(),
                FieldKind::Number(min, max, _) => {
                    let value = get_ctrl_text(hwnd, id);
                    value
                        .trim()
                        .parse::<u32>()
                        .ok()
                        .filter(|v| (min..=max).contains(v))
                        .ok_or_else(|| format!("{}应在 {} 到 {} 之间。", field.label, min, max))?
                        .to_string()
                }
                FieldKind::Text(_) => {
                    let value = get_ctrl_text(hwnd, id).trim().to_owned();
                    if matches!(
                        field.key,
                        Key::ListBackgroundColor | Key::GroupColor | Key::DefaultColor
                    ) && !value.is_empty()
                        && advanced::color(&value).is_none()
                    {
                        return Err(format!(
                            "{}请输入六位十六进制颜色，例如 #336699。",
                            field.label
                        ));
                    }
                    value
                }
                FieldKind::Skin => {
                    skin = SendMessageW(ctrl, CB_GETCURSEL, 0, 0).max(0) as usize;
                    continue;
                }
                FieldKind::Category => {
                    let index = SendMessageW(ctrl, LB_GETCURSEL, 0, 0) as usize;
                    categories
                        .get(index)
                        .map(|(id, _)| id.clone())
                        .unwrap_or_default()
                }
            };
            draft.set(field.key, value);
        }
    }
    if draft.number(Key::Browser) == 2 && draft.text(Key::CustomBrowser).is_empty() {
        return Err("请选择自定义浏览器的程序路径。".into());
    }
    Ok((draft, skin))
}

pub fn is_page_change(id: i32, notification: u32) -> bool {
    id == ID_PAGE && notification == LBN_SELCHANGE
}

fn opacity_status(value: isize) -> String {
    match value { 0 => "（透明）".into(), 100 => "（不透明）".into(), _ => format!("{}%", value) }
}

/// 箭头、轨道和拖动均同步更新草稿控件，确定后才提交设置。
pub unsafe fn update_opacity_value(hwnd: HWND, slider: HWND, message: usize) {
    let Some(index) = advanced::FIELDS[3].iter().position(|field| field.key == Key::ListOpacity) else { return; };
    if slider != 0 && slider == GetDlgItem(hwnd, field_id(3, index)) {
        let current = SendMessageW(slider, SBM_GETPOS, 0, 0);
        let value = match (message & 0xffff) as i32 {
            SB_LINELEFT => current - 1, SB_LINERIGHT => current + 1,
            SB_PAGELEFT => current - 10, SB_PAGERIGHT => current + 10,
            SB_THUMBTRACK | SB_THUMBPOSITION => ((message >> 16) & 0xffff) as isize,
            SB_LEFT => 10, SB_RIGHT => 100, _ => current,
        }.clamp(10, 100);
        SendMessageW(slider, SBM_SETPOS, value as usize, 1);
        set_ctrl_text(hwnd, ID_OPACITY_VALUE, &opacity_status(value));
    }
}

pub const TIME_FORMAT_HELP: &str = "week - 星期X\ntime - 时间\ny - 年\nm - 月\nd - 日\nww - 一年的第几周\nst - 开机了多久\n\\n - 回车符\n\\b - 空格\n\\* - 不显示时间标签";

unsafe fn install_icon_tip(owner:HWND,control:HWND,key:Key) {
    use windows_sys::Win32::{UI::{Controls::*,Shell::SetWindowSubclass},System::LibraryLoader::GetModuleHandleW};
    use std::{ptr,mem,sync::OnceLock};
    static TEXT:OnceLock<[Vec<u16>;2]>=OnceLock::new();
    let text=TEXT.get_or_init(||[to_wide("选择列表背景图片"),to_wide(TIME_FORMAT_HELP)]);
    let tip=windows_sys::Win32::UI::WindowsAndMessaging::CreateWindowExW(
        windows_sys::Win32::UI::WindowsAndMessaging::WS_EX_TOOLWINDOW | windows_sys::Win32::UI::WindowsAndMessaging::WS_EX_TOPMOST,
        to_wide("tooltips_class32").as_ptr(),ptr::null(),windows_sys::Win32::UI::WindowsAndMessaging::WS_POPUP|TTS_ALWAYSTIP|TTS_NOPREFIX,
        0,0,0,0,owner,0,GetModuleHandleW(ptr::null()),ptr::null());
    if tip!=0 {
        let mut tool:TTTOOLINFOW=mem::zeroed();tool.cbSize=mem::size_of::<TTTOOLINFOW>() as u32;
        tool.uFlags=TTF_IDISHWND|TTF_SUBCLASS;tool.hwnd=owner;tool.uId=control as usize;
        tool.lpszText=text[usize::from(key==Key::TimeFormat)].as_ptr().cast_mut();
        if SendMessageW(tip,TTM_ADDTOOLW,0,&tool as *const _ as isize)==0 {
            // 兼容未启用 v6 清单时的旧版公共控件 TOOLINFO 大小。
            tool.cbSize=std::mem::offset_of!(TTTOOLINFOW,lpReserved) as u32;
            SendMessageW(tip,TTM_ADDTOOLW,0,&tool as *const _ as isize);
        }
        SendMessageW(tip,TTM_SETTIPTEXTCOLOR,0x00800000,0);
        SendMessageW(tip,TTM_SETMAXTIPWIDTH,0,220);
        SendMessageW(tip,TTM_SETDELAYTIME,TTDT_INITIAL as usize,150);
        SendMessageW(tip,TTM_SETDELAYTIME,TTDT_AUTOPOP as usize,20000);
    }
    SetWindowSubclass(control,Some(icon_proc),1,0);
}

unsafe extern "system" fn icon_proc(hwnd:HWND,msg:u32,wp:usize,lp:isize,_:usize,_:usize)->isize {
    use windows_sys::Win32::UI::{Shell::{DefSubclassProc,RemoveWindowSubclass},WindowsAndMessaging::*};
    if msg==WM_SETCURSOR {SetCursor(LoadCursorW(0,IDC_HAND));return 1;}
    if msg==WM_NCDESTROY {RemoveWindowSubclass(hwnd,Some(icon_proc),1);}
    DefSubclassProc(hwnd,msg,wp,lp)
}

pub unsafe fn draw_picker_icon(draw:&windows_sys::Win32::UI::Controls::DRAWITEMSTRUCT)->bool {
    use windows_sys::Win32::{Graphics::Gdi::*,UI::{Controls::ODS_FOCUS,WindowsAndMessaging::*}};
    let Some((_,key))=picker(draw.CtlID as i32) else{return false;};
    if !matches!(key,Key::ListBackground|Key::TimeFormat){return false;}
    FillRect(draw.hDC,&draw.rcItem,crate::advanced_style::static_color(draw.hDC));
    let size=12;
    let x=draw.rcItem.left+(draw.rcItem.right-draw.rcItem.left-size)/2;
    let y=draw.rcItem.top+(draw.rcItem.bottom-draw.rcItem.top-size)/2;
    if key==Key::TimeFormat {
        let icon=LoadImageW(0,IDI_INFORMATION,IMAGE_ICON,size,size,0);
        if icon!=0 {DrawIconEx(draw.hDC,x,y,icon,size,size,0,0,DI_NORMAL);DestroyIcon(icon);}
    }else{
        static ICON:std::sync::OnceLock<Option<crate::skin_assets::RgbaSurface>>=std::sync::OnceLock::new();
        if let Some(icon)=ICON.get_or_init(||{
            let image=image::load_from_memory(include_bytes!("../assets/icons/ctx_add_file.png")).ok()?.to_rgba8();
            Some(crate::skin_assets::RgbaSurface::from_pixels(image.width(),image.height(),image.pixels().map(|p|p.0).collect()))
        }) {
            let color=crate::advanced_style::PAGE;
            crate::native_dialogs::blit_surface_on_bg(draw.hDC,icon,x,y,size,size,[color as u8,(color>>8) as u8,(color>>16) as u8]);
        }
    }
    if draw.itemState&ODS_FOCUS!=0{DrawFocusRect(draw.hDC,&draw.rcItem);}
    true
}
