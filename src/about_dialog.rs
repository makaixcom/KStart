//! 关于窗口：原生标题栏、链接、只读 EDIT、分组框和按钮。
use crate::{native_dialogs::{blit_surface_on_bg, create_ctrl}, APP_STATE};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    Graphics::Gdi::{FillRect, GetSysColor, GetSysColorBrush, COLOR_BTNFACE, HFONT},
    UI::{Controls::{DRAWITEMSTRUCT, ICC_LINK_CLASS, INITCOMMONCONTROLSEX, InitCommonControlsEx,
        NMHDR, NM_CLICK, NM_RETURN}, WindowsAndMessaging::*},
};

const BANNER: i32 = 1700;
const LINKS: i32 = 1701;
const SUPPORT: i32 = 1702;
const INTRO: i32 = 1703;

pub unsafe fn populate(hwnd: HWND, font: HFONT) {
    InitCommonControlsEx(&INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_LINK_CLASS,
    });
    create_ctrl(hwnd, font, "STATIC", "", 0, 0x000d, 8, 8, 137, 360, BANNER);
    create_ctrl(hwnd, font, "STATIC", &format!("版本号: v{}", env!("CARGO_PKG_VERSION")),
        0, 0, 160, 18, 350, 20, -1);
    create_ctrl(hwnd, font, "STATIC", "藏鲸阁软件工作室 版权所有(C)",
        0, 0, 160, 43, 350, 20, -1);
    create_ctrl(hwnd, font, "BUTTON", "联系我们", 0, BS_GROUPBOX as u32,
        155, 74, 355, 104, -1);
    create_ctrl(hwnd, font, "SysLink",
        "主页: <a href=\"http://www.makaix.com\">http://www.makaix.com</a>\r\n网盘: <a href=\"http://www.makaix.com\">http://www.makaix.com</a>\r\n微信群: <a href=\"http://www.makaix.com\">http://www.makaix.com</a>",
        0, WS_TABSTOP, 170, 96, 325, 70, LINKS);
    create_ctrl(hwnd, font, "STATIC", "软件介绍:", 0, 0, 155, 190, 355, 20, -1);
    create_ctrl(hwnd, font, "EDIT", DESCRIPTION, WS_EX_CLIENTEDGE,
        WS_TABSTOP | WS_VSCROLL | (ES_MULTILINE | ES_AUTOVSCROLL | ES_READONLY) as u32,
        155, 214, 355, 154, INTRO);
    create_ctrl(hwnd, font, "SysLink", "<a href=\"http://www.makaix.com\">点击这里，支持快速启动的发展</a>",
        0, WS_TABSTOP, 14, 379, 320, 22, SUPPORT);
    create_ctrl(hwnd, font, "BUTTON", "关闭", 0, WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        432, 377, 78, 26, IDCANCEL);
}

pub unsafe fn notify(hwnd: HWND, lp: LPARAM) -> bool {
    let Some(header) = (lp as *const NMHDR).as_ref() else { return false; };
    if !matches!(header.idFrom as i32, LINKS | SUPPORT)
        || !matches!(header.code, NM_CLICK | NM_RETURN) { return false; }
    // 仅打开固定链接；通知处理不持有 APP_STATE 借用。
    crate::launch_target_url_or_path("http://www.makaix.com", "", "", hwnd);
    true
}

pub unsafe fn draw_banner(draw: &DRAWITEMSTRUCT) -> bool {
    if draw.CtlID as i32 != BANNER { return false; }
    FillRect(draw.hDC, &draw.rcItem, GetSysColorBrush(COLOR_BTNFACE));
    APP_STATE.with(|cell| {
        let Ok(guard) = cell.try_borrow() else { return; };
        let Some(app) = guard.as_ref() else { return; };
        let [r, g, b, _] = GetSysColor(COLOR_BTNFACE).to_le_bytes();
        blit_surface_on_bg(draw.hDC, &app.builtin_icons.about_banner,
            draw.rcItem.left, draw.rcItem.top, draw.rcItem.right - draw.rcItem.left,
            draw.rcItem.bottom - draw.rcItem.top, [r, g, b]);
    });
    true
}

const DESCRIPTION: &str = "    欢迎使用快速启动。快速启动是一款完全免费的软件，不存在任何功能或时间限制。它具有以下激动人心的特性：\r\n\r\n\
        1) 超方便管理软件、文件夹、网址、以及强大的系统功能。\r\n\
        2) 无数量限制的热键设定，一键执行任何需要的功能。\r\n\
        3) 漂亮的OICQ式界面，强大的换肤功能(拥有众多皮肤)。\r\n\
        4) 低于2MB的内存消耗，零CPU占用，不增加系统负担。\r\n\
        5) 邮件检测、收集面板、天气预报等实用功能/插件。\r\n\r\n\
        \x20   如果您喜欢快速启动，请捐助我们；或介绍给您的朋友、或写文章投稿于报刊网站；另外，欢迎反馈任何的建议（留言簿或QQ）";
