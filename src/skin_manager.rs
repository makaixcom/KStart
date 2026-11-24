//! 皮肤管理器的本地/网络页、等比预览和共用平面滚动条。
use crate::{
    config::SkinEntry,
    native_dialogs::{self, create_ctrl},
    skin_assets::{Canvas, RgbaSurface},
    to_wide, AppState, APP_STATE,
};
use std::os::windows::fs::MetadataExt;
use std::{cell::RefCell, mem, path::PathBuf, ptr};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{
        Controls::*,
        Input::KeyboardAndMouse::{EnableWindow, GetCapture, ReleaseCapture, SetCapture},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

pub const TAB: i32 = 1004;
pub const DELETE: i32 = 1005;
pub const PREVIEW: i32 = 1006;
pub const SCROLL: i32 = 1007;
const DETAIL_SCROLL: i32 = 1008;
const TITLE: i32 = 1009;
const NETWORK: i32 = 1010;
const SUBCLASS: usize = 0x4b5353;
thread_local! {static PREVIEW_CACHE:RefCell<Option<(PathBuf,RgbaSurface)>>=const{RefCell::new(None)};}

pub unsafe fn populate(hwnd: HWND, font: HFONT, app: &AppState) {
    let tab = create_ctrl(
        hwnd,
        font,
        "SysTabControl32",
        "",
        0,
        WS_TABSTOP,
        12,
        8,
        164,
        28,
        TAB,
    );
    for (i, label) in ["本地", "网络"].into_iter().enumerate() {
        let mut text = to_wide(label);
        let mut item: TCITEMW = mem::zeroed();
        item.mask = TCIF_TEXT;
        item.pszText = text.as_mut_ptr();
        SendMessageW(tab, TCM_INSERTITEMW, i, &item as *const _ as LPARAM);
    }
    let list = create_ctrl(
        hwnd,
        font,
        "LISTBOX",
        "",
        0,
        WS_BORDER
            | WS_TABSTOP
            | (LBS_NOTIFY | LBS_NOINTEGRALHEIGHT | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS) as u32,
        12,
        38,
        152,
        588,
        native_dialogs::ID_SKIN_LIST,
    );
    SendMessageW(list, LB_SETITEMHEIGHT, 0, 14);
    for skin in &app.skins {
        SendMessageW(
            list,
            LB_ADDSTRING,
            0,
            to_wide(&skin.name).as_ptr() as LPARAM,
        );
    }
    SendMessageW(list, LB_SETCURSEL, app.selected_skin_index, 0);
    SetWindowSubclass(list, Some(source_proc), SUBCLASS, SCROLL as usize);
    create_ctrl(
        hwnd,
        font,
        "STATIC",
        "皮肤详情",
        0,
        0,
        206,
        10,
        500,
        22,
        TITLE,
    );
    create_ctrl(
        hwnd, font, "STATIC", "", 0, 0x000D, 206, 38, 540, 456, PREVIEW,
    );
    let details = app
        .skins
        .get(app.selected_skin_index)
        .map(details)
        .unwrap_or_default();
    let edit = create_ctrl(
        hwnd,
        font,
        "EDIT",
        &details,
        0,
        (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32,
        206,
        506,
        524,
        100,
        native_dialogs::ID_SKIN_AUTHOR_EDIT,
    );
    SetWindowSubclass(edit, Some(source_proc), SUBCLASS, DETAIL_SCROLL as usize);
    for id in [SCROLL, DETAIL_SCROLL] {
        let bar = create_ctrl(hwnd, font, "STATIC", "", 0, 0x000D, 164, 38, 14, 588, id);
        SetWindowSubclass(bar, Some(bar_proc), SUBCLASS, 0);
    }
    create_ctrl(
        hwnd,
        font,
        "STATIC",
        "网络皮肤源待开发",
        0,
        0,
        206,
        62,
        500,
        48,
        NETWORK,
    );
    ShowWindow(GetDlgItem(hwnd, NETWORK), SW_HIDE);
    create_ctrl(
        hwnd,
        font,
        "BUTTON",
        "应用(&A)",
        0,
        WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        576,
        606,
        78,
        26,
        native_dialogs::ID_SKIN_APPLY,
    );
    create_ctrl(
        hwnd,
        font,
        "BUTTON",
        "删除(&D)",
        0,
        WS_TABSTOP,
        664,
        606,
        78,
        26,
        DELETE,
    );
    create_ctrl(
        hwnd,
        font,
        "BUTTON",
        "关闭(&C)",
        0,
        WS_TABSTOP,
        664,
        642,
        78,
        26,
        IDCANCEL,
    );
    if let Some(skin) = app.skins.get(app.selected_skin_index) {
        update_title(hwnd, &skin.name);
    }
}

pub unsafe fn update_title(hwnd: HWND, name: &str) {
    native_dialogs::set_ctrl_text(hwnd, TITLE, &format!("皮肤详情：{name}"));
}

/// 缩略图只缩放到能完整容纳的尺寸，横纵比例始终保持一致。
pub fn fit_preview(source: (u32, u32), area: (i32, i32)) -> (i32, i32) {
    if source.0 == 0 || source.1 == 0 || area.0 <= 0 || area.1 <= 0 {
        return (0, 0);
    }
    let scale = (area.0 as f64 / source.0 as f64).min(area.1 as f64 / source.1 as f64);
    (
        (source.0 as f64 * scale).round().max(1.0) as i32,
        (source.1 as f64 * scale).round().max(1.0) as i32,
    )
}

pub fn details(entry: &SkinEntry) -> String {
    let mut author = None;
    let mut date = None;
    let mut description = Vec::new();
    for line in entry
        .author_info
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let split = line.split_once(['：', ':']);
        if let Some((key, value)) = split {
            match key.trim().to_lowercase().as_str() {
                "作者" | "皮肤作者" | "author" => {
                    author = Some(value.trim().to_string());
                    continue;
                }
                "时间" | "日期" | "制作时间" | "date" => {
                    date = Some(value.trim().to_string());
                    continue;
                }
                "描述" | "说明" | "description" => {
                    description.push(value.trim().to_string());
                    continue;
                }
                _ => {}
            }
        }
        if (line.starts_with("19") || line.starts_with("20"))
            && line.chars().filter(|c| c.is_ascii_digit()).count() >= 6
            && date.is_none()
        {
            date = Some(line.into());
        } else {
            description.push(line.into());
        }
    }
    format!(
        "作者：{}\r\n时间：{}\r\n描述：{}",
        author
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "未知".into()),
        date.filter(|s| !s.is_empty())
            .unwrap_or_else(|| "未知".into()),
        if description.is_empty() {
            "暂无描述".into()
        } else {
            description.join("\r\n")
        }
    )
}

pub unsafe fn arrange(hwnd: HWND, w: i32, h: i32, split: i32) {
    let move_to = |id, x, y, width, height| {
        SetWindowPos(
            GetDlgItem(hwnd, id),
            0,
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    };
    move_to(TAB, 12, 8, split - 24, 28);
    move_to(native_dialogs::ID_SKIN_LIST, 12, 38, split - 40, h - 92);
    move_to(SCROLL, split - 28, 38, 14, h - 92);
    move_to(TITLE, split + 12, 10, w - split - 24, 22);
    move_to(PREVIEW, split + 12, 38, w - split - 24, (h - 224).max(40));
    move_to(
        native_dialogs::ID_SKIN_AUTHOR_EDIT,
        split + 12,
        h - 174,
        w - split - 40,
        100,
    );
    move_to(DETAIL_SCROLL, w - 28, h - 174, 14, 100);
    move_to(NETWORK, split + 12, 62, w - split - 24, 48);
    move_to(native_dialogs::ID_SKIN_APPLY, w - 184, h - 74, 78, 26);
    move_to(DELETE, w - 96, h - 74, 78, 26);
    move_to(IDCANCEL, w - 96, h - 38, 78, 26);
}

pub unsafe fn paint_splitter(hwnd: HWND, dc: HDC) {
    let split = crate::dialog_layout::skin_split(hwnd);
    let mut client: RECT = mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let brush = CreateSolidBrush(0x00dc_d3c9);
    FillRect(
        dc,
        &RECT {
            left: split - 3,
            top: 8,
            right: split - 2,
            bottom: client.bottom - 50,
        },
        brush,
    );
    DeleteObject(brush);
}

pub unsafe fn notify(hwnd: HWND, param: LPARAM) -> bool {
    if param == 0 {
        return false;
    }
    let header = &*(param as *const NMHDR);
    if header.idFrom != TAB as usize || header.code != TCN_SELCHANGE {
        return false;
    }
    let local = SendMessageW(GetDlgItem(hwnd, TAB), TCM_GETCURSEL, 0, 0) == 0;
    for id in [
        native_dialogs::ID_SKIN_LIST,
        SCROLL,
        PREVIEW,
        native_dialogs::ID_SKIN_AUTHOR_EDIT,
        DETAIL_SCROLL,
    ] {
        ShowWindow(GetDlgItem(hwnd, id), if local { SW_SHOW } else { SW_HIDE });
    }
    ShowWindow(
        GetDlgItem(hwnd, NETWORK),
        if local { SW_HIDE } else { SW_SHOW },
    );
    for id in [native_dialogs::ID_SKIN_APPLY, DELETE] {
        EnableWindow(GetDlgItem(hwnd, id), local as i32);
    }
    let name = APP_STATE.with(|cell| {
        cell.try_borrow().ok().and_then(|guard| {
            let app = guard.as_ref()?;
            Some(app.skins.get(app.selected_skin_index)?.name.clone())
        })
    });
    if local {
        if let Some(name) = name {
            update_title(hwnd, &name);
        }
    } else {
        native_dialogs::set_ctrl_text(hwnd, TITLE, "网络皮肤");
    }
    InvalidateRect(hwnd, ptr::null(), 1);
    true
}

unsafe fn scroll_info(parent: HWND, bar: i32) -> (HWND, usize, usize, usize, i32) {
    let source = GetDlgItem(
        parent,
        if bar == SCROLL {
            native_dialogs::ID_SKIN_LIST
        } else {
            native_dialogs::ID_SKIN_AUTHOR_EDIT
        },
    );
    let mut rect: RECT = mem::zeroed();
    GetClientRect(source, &mut rect);
    let (count, top, line) = if bar == SCROLL {
        (
            SendMessageW(source, LB_GETCOUNT, 0, 0),
            SendMessageW(source, LB_GETTOPINDEX, 0, 0),
            SendMessageW(source, LB_GETITEMHEIGHT, 0, 0).max(1) as i32,
        )
    } else {
        (
            SendMessageW(source, EM_GETLINECOUNT, 0, 0),
            SendMessageW(source, EM_GETFIRSTVISIBLELINE, 0, 0),
            14,
        )
    };
    (
        source,
        count.max(0) as usize,
        ((rect.bottom - rect.top) / line).max(1) as usize,
        top.max(0) as usize,
        rect.bottom,
    )
}

pub unsafe fn draw(draw: &DRAWITEMSTRUCT, app: &AppState) -> bool {
    let id = draw.CtlID as i32;
    if ![native_dialogs::ID_SKIN_LIST, PREVIEW, SCROLL, DETAIL_SCROLL].contains(&id) {
        return false;
    }
    let background = crate::advanced_style::PAGE;
    let rgb = background.to_le_bytes();
    FillRect(
        draw.hDC,
        &draw.rcItem,
        crate::advanced_style::static_color(draw.hDC),
    );
    if id == native_dialogs::ID_SKIN_LIST {
        if let Some(skin) = app.skins.get(draw.itemID as usize) {
            let selected = draw.itemState & ODS_SELECTED != 0;
            if selected {
                FillRect(draw.hDC, &draw.rcItem, GetSysColorBrush(COLOR_HIGHLIGHT));
                SetTextColor(draw.hDC, GetSysColor(COLOR_HIGHLIGHTTEXT));
            }
            let mut area = draw.rcItem;
            area.left += 3;
            DrawTextW(
                draw.hDC,
                to_wide(&skin.name).as_ptr(),
                -1,
                &mut area,
                DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
            );
        }
    } else if id == PREVIEW {
        let image = app
            .skins
            .get(app.selected_skin_index)
            .and_then(|s| s.preview_gif_path.as_ref())
            .and_then(|path| {
                PREVIEW_CACHE.with(|cell| {
                    let mut cache = cell.try_borrow_mut().ok()?;
                    if cache.as_ref().is_none_or(|(old, _)| old != path) {
                        *cache =
                            RgbaSurface::load_file(path).map(|surface| (path.clone(), surface));
                    }
                    cache.as_ref().map(|(_, image)| image.clone())
                })
            });
        if let Some(image) = image {
            let area = (
                draw.rcItem.right - draw.rcItem.left,
                draw.rcItem.bottom - draw.rcItem.top,
            );
            let (w, h) = fit_preview((image.width, image.height), area);
            native_dialogs::blit_surface_on_bg(
                draw.hDC,
                &image,
                draw.rcItem.left + (area.0 - w) / 2,
                draw.rcItem.top + (area.1 - h) / 2,
                w,
                h,
                [rgb[0], rgb[1], rgb[2]],
            );
        } else {
            let mut rect = draw.rcItem;
            DrawTextW(
                draw.hDC,
                to_wide("暂无皮肤预览").as_ptr(),
                -1,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
    } else {
        let (_, total, visible, top, _) = scroll_info(GetParent(draw.hwndItem), id);
        if total > visible {
            let (w, h) = (
                draw.rcItem.right - draw.rcItem.left,
                draw.rcItem.bottom - draw.rcItem.top,
            );
            let mut canvas = Canvas::new(w, h);
            crate::renderer::draw_flat_scrollbar(&mut canvas, 0, 0, w, h, visible, total, top);
            let surface = RgbaSurface::from_pixels(
                w as u32,
                h as u32,
                canvas
                    .buffer
                    .iter()
                    .map(|p| {
                        let [b, g, r, _] = p.to_le_bytes();
                        [r, g, b, 255]
                    })
                    .collect(),
            );
            native_dialogs::blit_surface_on_bg(
                draw.hDC,
                &surface,
                0,
                0,
                w,
                h,
                [rgb[0], rgb[1], rgb[2]],
            );
        }
    }
    true
}

unsafe fn scroll_to(parent: HWND, bar: i32, position: usize) {
    let (source, total, visible, old, _) = scroll_info(parent, bar);
    let position = position.min(total.saturating_sub(visible));
    if bar == SCROLL {
        SendMessageW(source, LB_SETTOPINDEX, position, 0);
    } else {
        SendMessageW(source, EM_LINESCROLL, 0, position as isize - old as isize);
    }
    InvalidateRect(GetDlgItem(parent, bar), ptr::null(), 0);
}

unsafe extern "system" fn source_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    bar: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(source_proc), id);
        return DefSubclassProc(hwnd, msg, wp, lp);
    }
    if msg == WM_MOUSEWHEEL {
        let parent = GetParent(hwnd);
        let (_, _, _, top, _) = scroll_info(parent, bar as i32);
        scroll_to(
            parent,
            bar as i32,
            (top as isize - ((wp >> 16) as i16 as isize / 120) * 3).max(0) as usize,
        );
        return 0;
    }
    let result = DefSubclassProc(hwnd, msg, wp, lp);
    if [
        WM_KEYDOWN,
        WM_SIZE,
        WM_SETTEXT,
        LB_SETTOPINDEX,
        EM_LINESCROLL,
    ]
    .contains(&msg)
    {
        InvalidateRect(GetDlgItem(GetParent(hwnd), bar as i32), ptr::null(), 0);
    }
    result
}

unsafe extern "system" fn bar_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(bar_proc), id);
        RemovePropW(hwnd, to_wide("KStart.ScrollGrab").as_ptr());
        return DefSubclassProc(hwnd, msg, wp, lp);
    }
    let parent = GetParent(hwnd);
    let bar = GetDlgCtrlID(hwnd);
    if [
        WM_LBUTTONDOWN,
        WM_MOUSEMOVE,
        WM_LBUTTONUP,
        WM_CAPTURECHANGED,
    ]
    .contains(&msg)
    {
        let (_, total, visible, top, _) = scroll_info(parent, bar);
        let mut area: RECT = mem::zeroed();
        GetClientRect(hwnd, &mut area);
        let (y, _, travel) =
            crate::renderer::flat_scrollbar_metrics(area.bottom, visible, total, top);
        let (_, thumb, _) =
            crate::renderer::flat_scrollbar_metrics(area.bottom, visible, total, top);
        let pointer = ((lp >> 16) as i16) as i32;
        if msg == WM_LBUTTONDOWN && total > visible {
            if pointer < 14 {
                scroll_to(parent, bar, top.saturating_sub(1));
            } else if pointer >= area.bottom - 14 {
                scroll_to(parent, bar, top + 1);
            } else if pointer >= y && pointer < y + thumb {
                SetPropW(
                    hwnd,
                    to_wide("KStart.ScrollGrab").as_ptr(),
                    (pointer - y + 1) as isize,
                );
                SetCapture(hwnd);
            } else {
                scroll_to(
                    parent,
                    bar,
                    if pointer < y {
                        top.saturating_sub(visible)
                    } else {
                        top + visible
                    },
                );
            }
            return 0;
        }
        if msg == WM_MOUSEMOVE && GetCapture() == hwnd && travel > 0 {
            let grab = GetPropW(hwnd, to_wide("KStart.ScrollGrab").as_ptr()) as i32 - 1;
            let position = (pointer - 14 - grab).clamp(0, travel) as usize
                * total.saturating_sub(visible)
                / travel as usize;
            scroll_to(parent, bar, position);
            return 0;
        }
        if msg == WM_LBUTTONUP && GetCapture() == hwnd {
            ReleaseCapture();
            return 0;
        }
        if msg == WM_CAPTURECHANGED {
            RemovePropW(hwnd, to_wide("KStart.ScrollGrab").as_ptr());
        }
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

pub unsafe fn delete(hwnd: HWND) {
    let snapshot = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        Some((
            app.base_dir.clone(),
            app.skins.get(app.selected_skin_index)?.clone(),
            app.skins.len(),
            app.selected_skin_index == app.active_skin_index,
        ))
    });
    let Some((base, skin, count, active)) = snapshot else {
        return;
    };
    if count <= 1 {
        crate::advanced_runtime::message(hwnd, "至少需要保留一款皮肤。");
        return;
    }
    if !crate::advanced_runtime::confirm(hwnd, &format!("确定删除皮肤“{}”及其文件吗？", skin.name))
    {
        return;
    }
    let safe = (|| {
        let root = std::fs::canonicalize(base.join("skin")).ok()?;
        let target = std::fs::canonicalize(&skin.dir_path).ok()?;
        let metadata = std::fs::symlink_metadata(&skin.dir_path).ok()?;
        (target.parent() == Some(root.as_path()) && metadata.file_attributes() & 0x400 == 0)
            .then_some(())
    })();
    if safe.is_none() {
        crate::advanced_runtime::message(hwnd, "皮肤目录不在有效的本地皮肤根目录中。");
        return;
    }
    if active {
        let changed = APP_STATE.with(|cell| {
            let mut guard = cell.try_borrow_mut().ok()?;
            let app = guard.as_mut()?;
            let next = app.skins.iter().position(|s| s.name != skin.name)?;
            Some((app.switch_skin(next)?, app.main_hwnd))
        });
        let Some((region, main)) = changed else {
            crate::advanced_runtime::message(hwnd, "其他皮肤无法加载，已取消删除。");
            return;
        };
        crate::apply_hrgn_safely(main, region);
        InvalidateRect(main, ptr::null(), 0);
    }
    if let Err(error) = std::fs::remove_dir_all(&skin.dir_path) {
        crate::advanced_runtime::message(hwnd, &format!("删除失败：{error}"));
        return;
    }
    let updated = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        let active = app.skins.get(app.active_skin_index)?.name.clone();
        app.skins = crate::config::scan_skins(&base);
        app.active_skin_index = app.skins.iter().position(|s| s.name == active).unwrap_or(0);
        app.selected_skin_index = app
            .selected_skin_index
            .min(app.skins.len().saturating_sub(1));
        Some((app.skins.clone(), app.selected_skin_index))
    });
    if let Some((skins, selected)) = updated {
        let list = GetDlgItem(hwnd, native_dialogs::ID_SKIN_LIST);
        SendMessageW(list, LB_RESETCONTENT, 0, 0);
        for skin in &skins {
            SendMessageW(
                list,
                LB_ADDSTRING,
                0,
                to_wide(&skin.name).as_ptr() as LPARAM,
            );
        }
        SendMessageW(list, LB_SETCURSEL, selected, 0);
        if let Some(skin) = skins.get(selected) {
            update_title(hwnd, &skin.name);
            native_dialogs::set_ctrl_text(
                hwnd,
                native_dialogs::ID_SKIN_AUTHOR_EDIT,
                &details(skin),
            );
        }
        for id in [PREVIEW, SCROLL, DETAIL_SCROLL] {
            InvalidateRect(GetDlgItem(hwnd, id), ptr::null(), 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_preserves_ratio_for_wide_and_tall_images() {
        assert_eq!(fit_preview((400, 200), (300, 300)), (300, 150));
        assert_eq!(fit_preview((200, 400), (300, 300)), (150, 300));
        assert_eq!(fit_preview((0, 400), (300, 300)), (0, 0));
    }
    #[test]
    fn metadata_preserves_author_date_and_multiline_description() {
        let entry = SkinEntry {
            name: "测试".into(),
            dir_path: PathBuf::new(),
            setup_ini_path: PathBuf::new(),
            preview_gif_path: None,
            author_info: "作者：测试作者\n2007-12-17\n描述：第一行\n第二行".into(),
        };
        assert_eq!(
            details(&entry),
            "作者：测试作者\r\n时间：2007-12-17\r\n描述：第一行\r\n第二行"
        );
    }
}
