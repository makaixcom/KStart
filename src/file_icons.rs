//! Windows Shell 图标提取与用户图标来源加载。
use crate::surface::RgbaSurface;
use std::path::Path;

/// 调用 Windows Shell API 提取文件、目录或快捷方式的大尺寸图标。
/// 采用黑/白双底差值法，100% 精确还原 32bpp 透明通道与经典 1-bit AND 掩码黑色轮廓
#[cfg(windows)]
pub fn extract_file_icon_32(file_path: &Path) -> Option<RgbaSurface> {
    extract_file_icon_at_size(file_path, 32)
}

#[cfg(windows)]
fn extract_file_icon_at_size(file_path: &Path, size: u32) -> Option<RgbaSurface> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, SelectObject,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows_sys::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON};
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, DrawIconEx, DI_NORMAL};

    let wide: Vec<u16> = OsStr::new(file_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut shfi: SHFILEINFOW = std::mem::zeroed();
        let res = SHGetFileInfoW(
            wide.as_ptr(),
            0,
            &mut shfi,
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | if size == 16 { SHGFI_SMALLICON } else { 0 },
        );
        if res == 0 || shfi.hIcon == 0 {
            return None;
        }

        let render_on_bg = |bg_u32: u32| -> Option<Vec<u32>> {
            let hdc = CreateCompatibleDC(0);
            if hdc == 0 {
                return None;
            }
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size as i32,
                    biHeight: -(size as i32),
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
            let mut bits_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
            let hbm = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, 0, 0);
            if hbm == 0 || bits_ptr.is_null() {
                DeleteDC(hdc);
                return None;
            }
            let old_bm = SelectObject(hdc, hbm as _);
            std::slice::from_raw_parts_mut(bits_ptr as *mut u32, (size * size) as usize)
                .fill(bg_u32);
            DrawIconEx(
                hdc,
                0,
                0,
                shfi.hIcon,
                size as i32,
                size as i32,
                0,
                0,
                DI_NORMAL,
            );
            GdiFlush();
            let out =
                std::slice::from_raw_parts(bits_ptr as *const u32, (size * size) as usize).to_vec();
            SelectObject(hdc, old_bm);
            DeleteObject(hbm as _);
            DeleteDC(hdc);
            Some(out)
        };

        let black_buf = render_on_bg(0x0000_0000);
        let white_buf = render_on_bg(0x00FF_FFFF);
        DestroyIcon(shfi.hIcon);

        let (b_buf, w_buf) = match (black_buf, white_buf) {
            (Some(b), Some(w)) => (b, w),
            _ => return None,
        };

        let mut pixels = Vec::with_capacity((size * size) as usize);
        for i in 0..(size * size) as usize {
            let pb = b_buf[i];
            let pw = w_buf[i];
            let bb = (pb & 0xFF) as i32;
            let gb = ((pb >> 8) & 0xFF) as i32;
            let rb = ((pb >> 16) & 0xFF) as i32;

            let bw = (pw & 0xFF) as i32;
            let gw = ((pw >> 8) & 0xFF) as i32;
            let rw = ((pw >> 16) & 0xFF) as i32;

            let diff = ((rw - rb) + (gw - gb) + (bw - bb)) / 3;
            let alpha = (255 - diff).clamp(0, 255);
            if alpha == 0 {
                pixels.push([0, 0, 0, 0]);
            } else {
                let r = ((rb * 255) / alpha).clamp(0, 255) as u8;
                let g = ((gb * 255) / alpha).clamp(0, 255) as u8;
                let b = ((bb * 255) / alpha).clamp(0, 255) as u8;
                pixels.push([r, g, b, alpha as u8]);
            }
        }

        Some(RgbaSurface::from_pixels(size, size, pixels))
    }
}

#[cfg(all(test, windows))]
mod icon_tests {
    use super::*;

    #[test]
    fn shell_extraction_keeps_large_icon_pixels_and_transparency() {
        let path = std::env::current_exe().unwrap();
        let icon = extract_file_icon_32(&path).expect("应能提取当前程序的 Shell 图标");
        assert_eq!((icon.width, icon.height), (32, 32));
        assert_eq!(icon.pixels().len(), 1024);
        assert!(icon.pixels().iter().any(|pixel| pixel[3] > 0));
        assert!(icon.pixels().iter().any(|pixel| pixel[3] == 0));
    }
}

/// 加载用户自选图标，保留 32×32 细节；函数名兼容现有调用。
pub fn load_custom_icon_16(path: &Path) -> Option<RgbaSurface> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    #[cfg(windows)]
    if ext == "ico" {
        // 直接解码图标文件，避免 Shell 把 ICO 当作关联文件或提取失败。
        return RgbaSurface::load_file(path).map(|icon| icon.resize_to(32, 32))
            .or_else(|| extract_file_icon_32(path));
    }
    if matches!(ext.as_str(), "png" | "bmp" | "jpg" | "jpeg" | "gif" | "ico") {
        if let Some(surf) = RgbaSurface::load_file(path) {
            return Some(surf.resize_to(32, 32));
        }
    }
    #[cfg(windows)]
    {
        extract_file_icon_32(path)
    }
    #[cfg(not(windows))]
    None
}
