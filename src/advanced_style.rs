//! 高级设置的背景与字体、间距示意图；交互控件由 Windows 绘制。

use crate::{
    advanced::FontSetting, native_dialogs::blit_surface_on_bg, skin_assets::RgbaSurface, to_wide,
};
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW,
    FillRect, GetStockObject, SelectObject, SetBkMode, SetBkColor,
    SetTextColor, DEFAULT_CHARSET, DEFAULT_GUI_FONT, DT_CENTER, DT_END_ELLIPSIS, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, HBRUSH, HDC, HFONT,
    TRANSPARENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    u32::from_le_bytes([r, g, b, 0])
}

pub(crate) const PAGE: u32 = rgb(246, 248, 250);
const NAV: u32 = rgb(255, 255, 255);
const BORDER: u32 = rgb(201, 211, 220);
const INK: u32 = rgb(36, 51, 62);
const ACCENT: u32 = rgb(14, 113, 125);
const LIGHT_ACCENT: u32 = rgb(223, 242, 242);

const ICONS: [&[u8]; 7] = [
    include_bytes!("../assets/icons/menu_options.png"),
    include_bytes!("../assets/icons/im_props.png"),
    include_bytes!("../assets/icons/ctx_search.png"),
    include_bytes!("../assets/icons/im_edit.png"),
    include_bytes!("../assets/icons/menu_skin.png"),
    include_bytes!("../assets/icons/cat_laptop.png"),
    include_bytes!("../assets/icons/menu_schedule.png"),
];

fn icons() -> &'static [Option<RgbaSurface>; 7] {
    static IMAGES: OnceLock<[Option<RgbaSurface>; 7]> = OnceLock::new();
    IMAGES.get_or_init(|| {
        ICONS.map(|bytes| {
            let image = image::load_from_memory(bytes).ok()?.to_rgba8();
            Some(
                RgbaSurface::from_pixels(image.width(), image.height(), image.pixels().map(|p| p.0).collect())
                .resize_to(16, 16),
            )
        })
    })
}

const fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT {
        left,
        top,
        right,
        bottom,
    }
}

unsafe fn fill(dc: HDC, area: RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    if brush != 0 {
        FillRect(dc, &area, brush);
        DeleteObject(brush);
    }
}

unsafe fn boxed(dc: HDC, area: RECT, border: u32, background: u32) {
    fill(dc, area, border);
    fill(
        dc,
        rect(area.left + 1, area.top + 1, area.right - 1, area.bottom - 1),
        background,
    );
}

unsafe fn text(dc: HDC, font: HFONT, mut area: RECT, label: &[u16], color: u32, center: bool) {
    let old_font = if font != 0 { SelectObject(dc, font) } else { 0 };
    let old_mode = SetBkMode(dc, TRANSPARENT as i32);
    let old_color = SetTextColor(dc, color);
    let flags = DT_SINGLELINE
        | DT_VCENTER
        | DT_END_ELLIPSIS
        | DT_NOPREFIX
        | if center { DT_CENTER } else { 0 };
    DrawTextW(dc, label.as_ptr(), -1, &mut area, flags);
    SetTextColor(dc, old_color);
    SetBkMode(dc, old_mode);
    if old_font != 0 {
        SelectObject(dc, old_font);
    }
}

unsafe fn icon(dc: HDC, index: usize, x: i32, y: i32, background: u32) {
    if let Some(Some(icon)) = icons().get(index) {
        let [r, g, b, _] = background.to_le_bytes();
        blit_surface_on_bg(dc, icon, x, y, 16, 16, [r, g, b]);
    }
}

pub unsafe fn paint_panel(
    dc: HDC,
    font: HFONT,
    width: i32,
    height: i32,
    footer: i32,
    sections: impl Iterator<Item = (&'static str, i32)>,
) {
    fill(dc, rect(0, 0, width, height), PAGE);
    boxed(dc, rect(12, 12, 135, 386), BORDER, NAV);
    fill(dc, rect(13, 13, 134, 36), LIGHT_ACCENT);
    icon(dc, 0, 20, 16, LIGHT_ACCENT);
    text(
        dc,
        font,
        rect(44, 13, 130, 36),
        &to_wide("配置"),
        ACCENT,
        false,
    );
    fill(dc, rect(0, footer, width, footer + 1), BORDER);
    for (label, top) in sections {
        fill(dc, rect(145, top + 5, 148, top + 16), ACCENT);
        text(
            dc,
            font,
            rect(154, top, width - 14, top + 21),
            &to_wide(label),
            ACCENT,
            false,
        );
    }
}

pub unsafe fn paint_child(dc: HDC, width: i32, height: i32, footer: i32) {
    fill(dc, rect(0, 0, width, height), PAGE);
    fill(dc, rect(0, footer, width, footer + 1), BORDER);
}

pub unsafe fn paint_font_preview(dc: HDC, selection: &FontSetting) {
    let face = to_wide(&selection.face);
    let font = CreateFontW(
        -(selection.points * 96 / 72),
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        0,
        0,
        face.as_ptr(),
    );
    if font != 0 {
        text(
            dc,
            font,
            rect(64, 115, 374, 167),
            &to_wide("中文ABCabc"),
            ACCENT,
            false,
        );
        DeleteObject(font as _);
    }
}

pub unsafe fn paint_spacing_preview(dc: HDC, pixels: u32, mode: usize) {
    let area = rect(200, 37, 395, 187);
    boxed(dc, area, BORDER, NAV);
    let font = GetStockObject(DEFAULT_GUI_FONT) as HFONT;
    let cell = pixels.clamp(50, 500) as i32;
    let columns = ((area.right - area.left - 10) / cell).max(1);
    let row_height = if mode >= 2 { 35 } else { 23 };
    for (index, label) in ["快捷方式", "我的电脑", "文档", "浏览器", "工作目录", "设置"]
        .iter()
        .enumerate()
    {
        let index = index as i32;
        let left = area.left + 6 + index % columns * cell;
        let top = area.top + 5 + index / columns * row_height;
        if top + row_height > area.bottom {
            break;
        }
        let right = (left + cell - 4).min(area.right - 4);
        let icon_left = if mode == 2 || mode == 3 {
            (left + right - 16) / 2
        } else {
            left
        };
        icon(dc, 5, icon_left, top + 2, NAV);
        match mode {
            1 | 3 => {}
            2 => text(
                dc,
                font,
                rect(left, top + 16, right, top + row_height),
                &to_wide(label),
                INK,
                true,
            ),
            _ => text(
                dc,
                font,
                rect(left + 20, top, right, top + 21),
                &to_wide(label),
                INK,
                false,
            ),
        }
    }
}

pub unsafe fn static_color(dc: HDC) -> HBRUSH {
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, INK);
    SetBkColor(dc, PAGE);
    static BACKGROUND: OnceLock<isize> = OnceLock::new();
    *BACKGROUND.get_or_init(|| CreateSolidBrush(PAGE)) as HBRUSH
}

pub unsafe fn erase_background(hwnd: HWND, dc: HDC) {
    let mut bounds = rect(0, 0, 0, 0);
    if GetClientRect(hwnd, &mut bounds) != 0 { fill(dc, bounds, PAGE); }
}
