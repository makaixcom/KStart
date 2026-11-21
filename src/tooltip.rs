//! 简单提示条与标准提示卡共用独立、不抢焦点的窗口。
use crate::{advanced::Key, renderer::{ActivePopupMenu, HitTarget, TextAlign, TextDrawCmd},
    skin_assets::{Canvas, RgbaSurface}, skin_parser::RgbColor, to_wide, APP_STATE};
use std::{cell::RefCell, mem, ptr};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, ClientToScreen, EndPaint, GetMonitorInfoW,
    MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WIDTH: i32 = 230;
struct Frame { canvas: Canvas, texts: Vec<TextDrawCmd> }
thread_local! { static FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) }; }

fn text(text: String, x: i32, y: i32, width: i32, bold: bool, color: RgbColor) -> TextDrawCmd {
    TextDrawCmd { text, x, y, w: width, h: 18, color, bold, align: TextAlign::Left,
        mnemonic: false, multiline: false, font_slot: None }
}

fn frame(label: &str, standard: bool, icon: Option<&RgbaSurface>, bytes: Option<u64>) -> Frame {
    let mut fields = label.splitn(3, '\n');
    let title = fields.next().unwrap_or("");
    let path = fields.next().unwrap_or("");
    let remark = fields.next().unwrap_or("");
    let ink = RgbColor::new(0, 0, 0);
    let muted = RgbColor::new(100, 100, 100);
    let mut texts = Vec::new();
    if standard {
        let mut canvas = Canvas::new(WIDTH, 200);
        canvas.fill_rect(0, 0, WIDTH, 200, RgbColor::new(255, 255, 255));
        canvas.stroke_rect(0, 0, WIDTH, 200, RgbColor::new(126, 156, 195));
        canvas.fill_rect(1, 1, WIDTH - 2, 9, RgbColor::new(106, 143, 188));
        if let Some(icon) = icon { canvas.blit(&icon.resize_to(32, 32), 9, 23); }
        texts.push(text(title.into(), 47, 19, WIDTH - 57, true, ink));
        let paths = crate::renderer::wrap_tooltip_line(path, WIDTH - 57);
        for (row, line) in paths.into_iter().take(2).enumerate() {
            texts.push(text(line, 47, 37 + row as i32 * 16, WIDTH - 57, false, muted));
        }
        if let Some(bytes) = bytes {
            texts.push(text(format!("大小：{:.1} MB", bytes as f64 / 1_048_576.0),
                47, 70, WIDTH - 57, false, muted));
        }
        canvas.draw_hline(10, WIDTH - 11, 91, RgbColor::new(126, 156, 195));
        for (row, line) in crate::renderer::wrap_tooltip_line(remark, WIDTH - 24).into_iter().take(5).enumerate() {
            texts.push(text(line, 12, 98 + row as i32 * 18, WIDTH - 24, false, muted));
        }
        Frame { canvas, texts }
    } else {
        let width = label.lines().map(crate::renderer::estimate_text_px_width).max().unwrap_or(0)
            .saturating_add(24).clamp(80, WIDTH);
        let mut y = 7;
        for line in crate::renderer::wrap_tooltip_line(title, width - 20) {
            texts.push(text(line, 10, y, width - 20, true, ink)); y += 18;
        }
        for line in crate::renderer::wrap_tooltip_line(path, width - 20) {
            texts.push(text(line, 10, y, width - 20, false, ink)); y += 18;
        }
        let separator = y + 3;
        if !remark.is_empty() {
            y += 10;
            for line in crate::renderer::wrap_tooltip_line(remark, width - 20).into_iter().take(8) {
                texts.push(text(line, 10, y, width - 20, false, ink)); y += 18;
            }
        }
        let mut canvas = Canvas::new(width, y + 7);
        canvas.fill_rect(0, 0, width, y + 7, RgbColor::new(255, 255, 225));
        canvas.stroke_rect(0, 0, width, y + 7, ink);
        if !remark.is_empty() { canvas.draw_hline(10, width - 11, separator, muted); }
        Frame { canvas, texts }
    }
}

pub unsafe fn sync(owner: HWND) {
    let plan = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        let kind = app.user_cfg.advanced.number(Key::Tooltips);
        let desired = app.advanced_window.tooltip.as_ref().filter(|_| kind < 2
            && app.modal_hwnd == 0 && app.popup_menu == ActivePopupMenu::None
            && IsWindowVisible(owner) != 0 && IsIconic(owner) == 0)
            .map(|(x, y, label)| (*x, *y, label.clone(), kind));
        if desired == app.advanced_window.shown_tooltip { return None; }
        let old = app.advanced_window.tooltip_hwnd;
        app.advanced_window.tooltip_hwnd = 0;
        app.advanced_window.shown_tooltip = desired.clone();
        let icon = if let HitTarget::ShortcutItem(index) = app.hover_target {
            app.categories.get(app.active_category).and_then(|cat| cat.groups.get(cat.active_group_index))
                .and_then(|group| group.items.get(index)).and_then(|item| item.custom_icon_16.as_ref()
                    .or_else(|| app.builtin_icons.app_icons.get(&item.icon_key))).cloned()
        } else { None };
        Some((old, desired, icon))
    });
    let Some((old, desired, icon)) = plan else { return; };
    if old != 0 { DestroyWindow(old); }
    FRAME.with(|slot| { if let Ok(mut slot) = slot.try_borrow_mut() { *slot = None; } });
    let Some((x, y, label, kind)) = desired else { return; };
    let bytes = label.split('\n').nth(1).and_then(|path| std::fs::metadata(path).ok())
        .filter(|meta| meta.is_file()).map(|meta| meta.len());
    let view = frame(&label, kind == 1, icon.as_ref(), bytes);
    let (width, height) = (view.canvas.width, view.canvas.height);
    let mut origin = POINT { x, y };
    ClientToScreen(owner, &mut origin);
    let anchor = RECT { left: origin.x, top: origin.y, right: origin.x + 1, bottom: origin.y + 1 };
    let mut monitor: MONITORINFO = mem::zeroed();
    monitor.cbSize = mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST), &mut monitor) != 0 {
        origin.x = origin.x.clamp(monitor.rcWork.left, (monitor.rcWork.right - width).max(monitor.rcWork.left));
        origin.y = origin.y.clamp(monitor.rcWork.top, (monitor.rcWork.bottom - height).max(monitor.rcWork.top));
    }
    FRAME.with(|slot| { if let Ok(mut slot) = slot.try_borrow_mut() { *slot = Some(view); } });
    let class = to_wide("KStartShortcutTooltipClass");
    let instance = GetModuleHandleW(ptr::null());
    RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(wnd_proc), hInstance: instance,
        lpszClassName: class.as_ptr(), ..mem::zeroed() });
    let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TRANSPARENT,
        class.as_ptr(), ptr::null(), WS_POPUP, origin.x, origin.y, width, height,
        owner, 0, instance, ptr::null());
    if hwnd != 0 {
        APP_STATE.with(|cell| { if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() { app.advanced_window.tooltip_hwnd = hwnd; }
        }});
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            FRAME.with(|slot| { if let Ok(slot) = slot.try_borrow() {
                if let Some(view) = slot.as_ref() {
                    crate::blit_canvas_with_gdi_texts(&view.canvas, &view.texts, None, dc, false, None);
                }
            }});
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modes_have_a_fixed_maximum_width_and_distinct_layouts() {
        let label = format!("软件标题\nD:\\{}\\app.exe\n多行备注", "很长的路径".repeat(12));
        let simple = frame(&label, false, None, None);
        let standard = frame(&label, true, None, Some(1_048_576));
        assert!(simple.canvas.width <= WIDTH);
        assert_eq!((standard.canvas.width, standard.canvas.height), (WIDTH, 200));
        assert!(simple.texts.iter().any(|text| text.text.contains("备注")));
        assert!(standard.texts.iter().any(|text| text.text.contains("1.0 MB")));
        if let Some(directory) = std::env::var_os("KSTART_TOOLTIP_PREVIEW_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let icon = RgbaSurface::load_file(std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"), "/assets/icons/app_7zfm.png")));
            for (name, standard) in [("simple.png", false), ("standard.png", true)] {
                let preview = frame("7zFM\nE:\\soft\\7-Zip\\7zFM.exe\n这是备注\n第二行备注", standard, icon.as_ref(), Some(3_400_000));
                let pixels = crate::blit_canvas_with_gdi_texts(&preview.canvas, &preview.texts,
                    None, 0, true, None).unwrap();
                crate::save_bgra_to_png(preview.canvas.width as u32, preview.canvas.height as u32,
                    pixels, &directory.join(name)).unwrap();
            }
        }
    }
}
