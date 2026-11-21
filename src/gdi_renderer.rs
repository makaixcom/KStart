//! GDI 文字合成、像素呈现与截图导出，不访问应用状态。
use crate::{
    advanced,
    canvas::Canvas,
    renderer::{PopupTextClip, TextAlign, TextDrawCmd},
    to_wide,
};
use std::{cell::RefCell, path::Path, ptr};
use windows_sys::Win32::{Foundation::RECT, Graphics::Gdi::*};

#[derive(Default)]
struct FontCache {
    entries: Vec<(String, i32, bool, HFONT)>,
}

impl FontCache {
    unsafe fn get(&mut self, face: &str, height: i32, bold: bool) -> HFONT {
        if let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.0 == face && entry.1 == height && entry.2 == bold)
        {
            return entry.3;
        }
        let handle = CreateFontW(
            height,
            0,
            0,
            0,
            if bold { FW_BOLD } else { FW_NORMAL } as i32,
            0,
            0,
            0,
            GB2312_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            0,
            DEFAULT_QUALITY as u32,
            0,
            to_wide(face).as_ptr(),
        );
        if handle != 0 {
            self.entries.push((face.into(), height, bold, handle));
        }
        handle
    }

    fn clear(&mut self) {
        for (_, _, _, handle) in self.entries.drain(..) {
            unsafe {
                DeleteObject(handle);
            }
        }
    }
}

impl Drop for FontCache {
    fn drop(&mut self) {
        self.clear();
    }
}
thread_local! { static FONTS: RefCell<FontCache> = RefCell::default(); }

/// 通用 GDI DIBSection + 12px 宋体光栅化与呈现函数（供主窗体与独立弹窗共同复用）
pub(crate) fn blit_canvas_with_gdi_texts(
    canvas: &Canvas,
    texts: &[TextDrawCmd],
    popup_clip: Option<PopupTextClip>,
    target_hdc: HDC,
    capture_pixels: bool,
    settings_fonts: Option<&[advanced::FontSetting; 4]>,
) -> Option<Vec<u32>> {
    FONTS.with(|cache| {
        let mut font_cache = cache.try_borrow_mut().ok()?;
        // 仅保留一小组字体，设置频繁变化时在新帧开始前释放旧句柄。
        if font_cache.entries.len() > 16 {
            font_cache.clear();
        }
        let w = canvas.width;
        let h = canvas.height;
        unsafe {
            let mem_dc = CreateCompatibleDC(target_hdc);
            if mem_dc == 0 {
                return None;
            }

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [std::mem::zeroed()],
            };

            let mut bits_ptr: *mut core::ffi::c_void = ptr::null_mut();
            let hbm = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, 0, 0);
            if hbm == 0 || bits_ptr.is_null() {
                if hbm != 0 {
                    DeleteObject(hbm);
                }
                DeleteDC(mem_dc);
                return None;
            }

            let old_bm = SelectObject(mem_dc, hbm as _);
            let pixel_count = (w * h) as usize;
            std::slice::from_raw_parts_mut(bits_ptr as *mut u32, pixel_count)
                .copy_from_slice(&canvas.buffer[..pixel_count]);

            let hfont_normal = font_cache.get("宋体", -12, false);
            let hfont_bold = font_cache.get("宋体", -12, true);

            SetBkMode(mem_dc, TRANSPARENT as i32);
            let old_font = SelectObject(mem_dc, hfont_normal as _);
            let mut custom_fonts = [0; 4];
            if let Some(fonts) = settings_fonts {
                for (index, font) in fonts.iter().enumerate() {
                    if font.face == "宋体" && font.points == 9 {
                        continue;
                    }
                    custom_fonts[index] =
                        font_cache.get(&font.face, -(font.points.clamp(5, 48) * 96 / 72), false);
                }
            }

            let mut wide_text = Vec::new();
            let mut draw_text = |cmd: &TextDrawCmd| {
                let font = cmd
                    .font_slot
                    .and_then(|index| custom_fonts.get(index).copied())
                    .filter(|&font| font != 0);
                SelectObject(
                    mem_dc,
                    if let Some(font) = font {
                        font
                    } else if cmd.bold {
                        hfont_bold as _
                    } else {
                        hfont_normal as _
                    },
                );
                SetTextColor(mem_dc, cmd.color.to_colorref());

                let mut rc = RECT {
                    left: cmd.x,
                    top: cmd.y,
                    right: cmd.x + cmd.w,
                    bottom: cmd.y + cmd.h,
                };
                wide_text.clear();
                wide_text.extend(cmd.text.encode_utf16());
                wide_text.push(0);
                let mut flags = if cmd.multiline {
                    DT_WORDBREAK
                } else {
                    DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS
                };
                flags |= match cmd.align {
                    TextAlign::Left => DT_LEFT,
                    TextAlign::Center => DT_CENTER,
                };
                if !cmd.mnemonic {
                    flags |= DT_NOPREFIX;
                }
                DrawTextW(
                    mem_dc,
                    wide_text.as_ptr(),
                    (wide_text.len().saturating_sub(1)) as i32,
                    &mut rc,
                    flags,
                );
            };

            if let Some(clip) = popup_clip {
                let mut start = 0;
                for (layer, &(_, end)) in clip.layers.iter().enumerate() {
                    // 只裁去覆盖区域内的像素，不删除整条文字，也不改变居中位置。
                    for &((x, y, width, height), _) in &clip.layers[layer..] {
                        ExcludeClipRect(mem_dc, x, y, x + width, y + height);
                    }
                    texts[start..end].iter().for_each(&mut draw_text);
                    SelectClipRgn(mem_dc, 0);
                    start = end;
                }
                SelectClipRgn(mem_dc, 0);
                texts[start..].iter().for_each(&mut draw_text);
            } else {
                texts.iter().for_each(&mut draw_text);
            }

            if target_hdc != 0 {
                BitBlt(target_hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);
            }

            let captured = if capture_pixels {
                GdiFlush();
                Some(std::slice::from_raw_parts(bits_ptr as *const u32, pixel_count).to_vec())
            } else {
                None
            };

            SelectObject(mem_dc, old_font);
            SelectObject(mem_dc, old_bm);
            DeleteObject(hbm as _);
            DeleteDC(mem_dc);

            captured
        }
    })
}

/// 将 `Canvas` 与捕获到的像素保存为 PNG 截图文件
pub(crate) fn save_bgra_to_png(
    w: u32,
    h: u32,
    bgra: Vec<u32>,
    out_path: &Path,
) -> Result<(), String> {
    let mut rgba_bytes = Vec::with_capacity((w * h * 4) as usize);
    for px in bgra {
        let b = (px & 0xFF) as u8;
        let g = ((px >> 8) & 0xFF) as u8;
        let r = ((px >> 16) & 0xFF) as u8;
        rgba_bytes.extend_from_slice(&[r, g, b, 255]);
    }
    let img_buf = image::RgbaImage::from_raw(w, h, rgba_bytes).ok_or("创建 RgbaImage 失败")?;
    img_buf
        .save(out_path)
        .map_err(|e| format!("保存截图失败: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_frames_reuse_a_bounded_set_of_valid_fonts() {
        let canvas = Canvas::new(80, 30);
        let texts = [TextDrawCmd {
            text: "字体复用".into(),
            x: 0,
            y: 0,
            w: 80,
            h: 20,
            color: crate::skin_parser::RgbColor::new(0, 0, 0),
            bold: false,
            align: TextAlign::Left,
            mnemonic: false,
            multiline: false,
            font_slot: Some(0),
        }];
        for points in 5..40 {
            let mut fonts = std::array::from_fn(|_| advanced::FontSetting::default());
            fonts[0].face = "Arial".into();
            fonts[0].points = points;
            assert!(
                blit_canvas_with_gdi_texts(&canvas, &texts, None, 0, true, Some(&fonts)).is_some()
            );
        }
        FONTS.with(|cache| {
            let mut cache = cache.try_borrow_mut().unwrap();
            assert!(cache.entries.len() <= 22);
            assert!(cache
                .entries
                .iter()
                .all(|entry| unsafe { GetObjectType(entry.3) == OBJ_FONT as u32 }));
            cache.clear();
            assert!(cache.entries.is_empty());
        });
    }
}
