//! 皮肤贴图资源加载、系统图标提取与 2D 像素级画布渲染引擎
//! 支持 BMP / ICO / GIF / PNG 加载，以及从外部拖入的文件/快捷方式提取系统图标。

use crate::skin_parser::{RgbColor, SkinConfig, ToolButtonKind};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// RGBA 图像表面
#[derive(Debug, Clone)]
pub struct RgbaSurface {
    pub width: u32,
    pub height: u32,
    /// 像素数组，按行优先存储 `[R, G, B, A]`
    pub pixels: Vec<[u8; 4]>,
}

impl RgbaSurface {
    /// 创建指定尺寸的空白透明表面
    pub fn new_empty(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![[0, 0, 0, 0]; (width * height) as usize],
        }
    }

    /// 从图像文件加载（优先使用内置 ICO 解析器处理非标 `.ico`，其余使用 `image` 库）
    pub fn load_file(path: &Path) -> Option<Self> {
        let bytes = fs::read(path).ok()?;

        if path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("ico"))
            .unwrap_or(false)
        {
            if let Some(surf) = parse_ico_bytes(&bytes) {
                return Some(surf);
            }
        }

        let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
        let (width, height) = img.dimensions();
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for p in img.pixels() {
            pixels.push(p.0);
        }
        Some(Self {
            width,
            height,
            pixels,
        })
    }

    /// 将当前 RGBA 表面保存为 PNG 文件
    pub fn save_png(&self, path: &Path) -> bool {
        let mut raw = Vec::with_capacity((self.width * self.height * 4) as usize);
        for p in &self.pixels {
            raw.extend_from_slice(p);
        }
        if let Some(buf) = image::RgbaImage::from_raw(self.width, self.height, raw) {
            buf.save(path).is_ok()
        } else {
            false
        }
    }

    /// 缩放到指定尺寸（用于把用户自选图片缩放到 `16x16` 快捷方式图标）
    pub fn resize_to(&self, target_w: u32, target_h: u32) -> Self {
        if self.width == target_w && self.height == target_h {
            return self.clone();
        }
        let mut raw = Vec::with_capacity((self.width * self.height * 4) as usize);
        for p in &self.pixels {
            raw.extend_from_slice(p);
        }
        if let Some(buf) = image::RgbaImage::from_raw(self.width, self.height, raw) {
            let resized = image::imageops::resize(
                &buf,
                target_w,
                target_h,
                image::imageops::FilterType::Lanczos3,
            );
            let pixels = resized.pixels().map(|p| p.0).collect();
            return Self {
                width: target_w,
                height: target_h,
                pixels,
            };
        }
        Self::new_empty(target_w, target_h)
    }

    /// 获取 `(x, y)` 处的像素
    #[inline]
    pub fn get_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize]
        } else {
            [0, 0, 0, 0]
        }
    }

    /// 将指定矩形范围内的 `mask_color` 像素置为完全透明（用于窗口四角异形圆角裁剪）
    pub fn apply_color_key_in_rect(
        &mut self,
        rx: u32,
        ry: u32,
        rw: u32,
        rh: u32,
        mask_color: RgbColor,
    ) {
        let x_end = (rx + rw).min(self.width);
        let y_end = (ry + rh).min(self.height);
        for y in ry..y_end {
            for x in rx..x_end {
                let idx = (y * self.width + x) as usize;
                let p = self.pixels[idx];
                if p[0] == mask_color.r && p[1] == mask_color.g && p[2] == mask_color.b {
                    self.pixels[idx][3] = 0;
                }
            }
        }
    }
}

/// 调用 Windows Shell API (`SHGetFileInfoW`) 提取任意外部文件、目录或 `.lnk` 快捷方式的 `16x16` 图标
/// 采用黑/白双底差值法，100% 精确还原 32bpp 透明通道与经典 1-bit AND 掩码黑色轮廓
#[cfg(windows)]
pub fn extract_file_icon_16(file_path: &Path) -> Option<RgbaSurface> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
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
            SHGFI_ICON | SHGFI_SMALLICON,
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
                    biWidth: 16,
                    biHeight: -16,
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
            let slice = std::slice::from_raw_parts_mut(bits_ptr as *mut u32, 16 * 16);
            slice.fill(bg_u32);
            DrawIconEx(hdc, 0, 0, shfi.hIcon, 16, 16, 0, 0, DI_NORMAL);
            let out = slice.to_vec();
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

        let mut pixels = Vec::with_capacity(16 * 16);
        for i in 0..(16 * 16) {
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

        Some(RgbaSurface {
            width: 16,
            height: 16,
            pixels,
        })
    }
}

/// 从用户自选的图标来源（支持程序 `.exe`/`.dll`/`.ico`/`.lnk` 或图片 `.png`/`.bmp`/`.jpg`/`.gif`）加载 `16x16` 图标
pub fn load_custom_icon_16(path: &Path) -> Option<RgbaSurface> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if matches!(ext.as_str(), "png" | "bmp" | "jpg" | "jpeg" | "gif" | "ico") {
        if let Some(surf) = RgbaSurface::load_file(path) {
            return Some(surf.resize_to(16, 16));
        }
    }
    #[cfg(windows)]
    {
        extract_file_icon_16(path)
    }
    #[cfg(not(windows))]
    None
}

/// 解析 Windows `.ico` 文件字节流
fn parse_ico_bytes(data: &[u8]) -> Option<RgbaSurface> {
    if data.len() < 22 {
        return None;
    }
    let reserved = u16::from_le_bytes([data[0], data[1]]);
    let ico_type = u16::from_le_bytes([data[2], data[3]]);
    let count = u16::from_le_bytes([data[4], data[5]]);
    if reserved != 0 || ico_type != 1 || count == 0 {
        return None;
    }

    let entry_w = if data[6] == 0 { 256u32 } else { data[6] as u32 };
    let entry_h = if data[7] == 0 { 256u32 } else { data[7] as u32 };
    let bytes_in_res = u32::from_le_bytes([data[14], data[15], data[16], data[17]]) as usize;
    let image_offset = u32::from_le_bytes([data[18], data[19], data[20], data[21]]) as usize;

    if image_offset + bytes_in_res > data.len() || bytes_in_res < 40 {
        return None;
    }

    let dib = &data[image_offset..image_offset + bytes_in_res];
    if dib.starts_with(&[0x89, b'P', b'N', b'G']) {
        let img = image::load_from_memory(dib).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let pixels = img.pixels().map(|p| p.0).collect();
        return Some(RgbaSurface {
            width: w,
            height: h,
            pixels,
        });
    }

    let header_size = u32::from_le_bytes([dib[0], dib[1], dib[2], dib[3]]) as usize;
    if header_size < 40 || dib.len() < header_size {
        return None;
    }

    let bmp_w = i32::from_le_bytes([dib[4], dib[5], dib[6], dib[7]]);
    let bmp_h = i32::from_le_bytes([dib[8], dib[9], dib[10], dib[11]]);
    let bpp = u16::from_le_bytes([dib[14], dib[15]]);
    let clr_used = u32::from_le_bytes([dib[32], dib[33], dib[34], dib[35]]) as usize;

    let width = if bmp_w > 0 { bmp_w as u32 } else { entry_w };
    let height = if bmp_h > 0 {
        (bmp_h / 2) as u32
    } else {
        entry_h
    };
    if width == 0 || height == 0 || width > 512 || height > 512 {
        return None;
    }

    let palette_entries = if bpp <= 8 {
        if clr_used > 0 {
            clr_used
        } else {
            1usize << bpp
        }
    } else {
        0
    };

    let palette_offset = header_size;
    let xor_offset = palette_offset + palette_entries * 4;
    if xor_offset > dib.len() {
        return None;
    }

    let xor_stride = ((width as usize * bpp as usize + 31) / 32) * 4;
    let xor_size = xor_stride * height as usize;
    let and_stride = ((width as usize + 31) / 32) * 4;
    let and_offset = xor_offset + xor_size;

    if xor_offset + xor_size > dib.len() {
        return None;
    }

    let mut pixels = vec![[0u8; 4]; (width * height) as usize];

    for y in 0..height {
        let src_row = (height - 1 - y) as usize;
        let xor_row =
            &dib[xor_offset + src_row * xor_stride..xor_offset + (src_row + 1) * xor_stride];
        let and_row = if and_offset + (src_row + 1) * and_stride <= dib.len() {
            Some(&dib[and_offset + src_row * and_stride..and_offset + (src_row + 1) * and_stride])
        } else {
            None
        };

        for x in 0..width {
            let xi = x as usize;
            let (r, g, b, mut a) = match bpp {
                32 => {
                    let b = xor_row[xi * 4];
                    let g = xor_row[xi * 4 + 1];
                    let r = xor_row[xi * 4 + 2];
                    let a = xor_row[xi * 4 + 3];
                    (r, g, b, a)
                }
                24 => {
                    let b = xor_row[xi * 3];
                    let g = xor_row[xi * 3 + 1];
                    let r = xor_row[xi * 3 + 2];
                    (r, g, b, 255)
                }
                8 => {
                    let idx = xor_row[xi] as usize;
                    let p_off = palette_offset + idx * 4;
                    let b = dib[p_off];
                    let g = dib[p_off + 1];
                    let r = dib[p_off + 2];
                    (r, g, b, 255)
                }
                4 => {
                    let byte = xor_row[xi / 2];
                    let idx = if xi % 2 == 0 {
                        (byte >> 4) as usize
                    } else {
                        (byte & 0x0F) as usize
                    };
                    let p_off = palette_offset + idx * 4;
                    let b = dib[p_off];
                    let g = dib[p_off + 1];
                    let r = dib[p_off + 2];
                    (r, g, b, 255)
                }
                1 => {
                    let bit = (xor_row[xi / 8] >> (7 - (xi % 8))) & 1;
                    let p_off = palette_offset + (bit as usize) * 4;
                    let b = dib[p_off];
                    let g = dib[p_off + 1];
                    let r = dib[p_off + 2];
                    (r, g, b, 255)
                }
                _ => return None,
            };

            if let Some(arow) = and_row {
                let mask_bit = (arow[xi / 8] >> (7 - (xi % 8))) & 1;
                if mask_bit == 1 && (bpp < 32 || a == 0) {
                    a = 0;
                } else if bpp == 32 && a == 0 && mask_bit == 0 {
                    a = 255;
                }
            }

            pixels[(y * width + x) as usize] = [r, g, b, a];
        }
    }

    Some(RgbaSurface {
        width,
        height,
        pixels,
    })
}

/// 按钮三态交互状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetState {
    Normal,
    Hover,
    Down,
}

/// 三态图像资源（常态、悬停、按下）
#[derive(Debug, Clone)]
pub struct TriStateImage {
    pub normal: RgbaSurface,
    pub hover: Option<RgbaSurface>,
    pub down: Option<RgbaSurface>,
}

impl TriStateImage {
    /// 根据前缀名称加载三态图像
    pub fn load(skin_dir: &Path, prefix: &str, ext: &str) -> Option<Self> {
        let normal = RgbaSurface::load_file(&skin_dir.join(format!("{}_Normal.{}", prefix, ext)))?;
        let hover = RgbaSurface::load_file(&skin_dir.join(format!("{}_Move.{}", prefix, ext)));
        let down = RgbaSurface::load_file(&skin_dir.join(format!("{}_Down.{}", prefix, ext)));
        Some(Self {
            normal,
            hover,
            down,
        })
    }

    /// 根据当前状态获取对应图像表面
    pub fn get(&self, state: WidgetState) -> &RgbaSurface {
        match state {
            WidgetState::Normal => &self.normal,
            WidgetState::Hover => self.hover.as_ref().unwrap_or(&self.normal),
            WidgetState::Down => self
                .down
                .as_ref()
                .or(self.hover.as_ref())
                .unwrap_or(&self.normal),
        }
    }
}

/// 皮肤完整图像资源集合
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SkinAssets {
    pub skin_left: RgbaSurface,
    pub skin_top: RgbaSurface,
    pub skin_right: RgbaSurface,
    pub skin_bottom: RgbaSurface,
    pub skin_top_left: RgbaSurface,
    pub skin_top_right: RgbaSurface,
    pub skin_bottom_left: RgbaSurface,
    pub skin_bottom_right: RgbaSurface,

    pub btn_min: TriStateImage,
    pub btn_skin: TriStateImage,
    pub btn_close: TriStateImage,

    pub preview_base: RgbaSurface,
    pub category_tab: TriStateImage,
    pub search_btn: Option<TriStateImage>,

    pub group_bar: RgbaSurface,
    pub scroll_thumb: Option<RgbaSurface>,
    pub up_down_arrows: Option<RgbaSurface>,

    pub menu_left_strip: RgbaSurface,
    pub menu_check: RgbaSurface,

    pub tool_buttons: HashMap<ToolButtonKind, TriStateImage>,
    pub preview_thumbnail: Option<RgbaSurface>,
}

impl SkinAssets {
    /// 从皮肤目录加载所有贴图资源
    pub fn load(skin_dir: &Path, cfg: &SkinConfig) -> Result<Self, String> {
        let load_req = |name: &str| -> Result<RgbaSurface, String> {
            RgbaSurface::load_file(&skin_dir.join(name))
                .ok_or_else(|| format!("缺少皮肤贴图文件: {}", name))
        };

        let skin_left = load_req("Skin-1.bmp")?;
        let skin_top = load_req("Skin-2.bmp")?;
        let skin_right = load_req("Skin-3.bmp")?;
        let skin_bottom = load_req("Skin-4.bmp")?;
        let mut skin_top_left = load_req("Skin-5.bmp")?;
        let mut skin_top_right = load_req("Skin-6.bmp")?;
        let mut skin_bottom_left = load_req("Skin-7.bmp")?;
        let mut skin_bottom_right = load_req("Skin-8.bmp")?;

        let mc = cfg.window_mask_color;
        let cm = &cfg.corner_mask;
        skin_top_left.apply_color_key_in_rect(0, 0, cm.top_left_width, cm.top_left_height, mc);

        let tr_x = skin_top_right.width.saturating_sub(cm.top_right_width);
        skin_top_right.apply_color_key_in_rect(
            tr_x,
            0,
            cm.top_right_width,
            cm.top_right_height,
            mc,
        );

        let bl_y = skin_bottom_left.height.saturating_sub(cm.bottom_left_height);
        skin_bottom_left.apply_color_key_in_rect(
            0,
            bl_y,
            cm.bottom_left_width,
            cm.bottom_left_height,
            mc,
        );

        let br_x = skin_bottom_right
            .width
            .saturating_sub(cm.bottom_right_width);
        let br_y = skin_bottom_right
            .height
            .saturating_sub(cm.bottom_right_height);
        skin_bottom_right.apply_color_key_in_rect(
            br_x,
            br_y,
            cm.bottom_right_width,
            cm.bottom_right_height,
            mc,
        );

        let btn_min = TriStateImage::load(skin_dir, "Min", "bmp")
            .ok_or_else(|| "缺少 Min_Normal.bmp".to_string())?;
        let btn_skin = TriStateImage::load(skin_dir, "Skin", "bmp")
            .ok_or_else(|| "缺少 Skin_Normal.bmp".to_string())?;
        let btn_close = TriStateImage::load(skin_dir, "Close", "bmp")
            .ok_or_else(|| "缺少 Close_Normal.bmp".to_string())?;

        let preview_base = load_req("S_Normal.bmp")?;
        let category_tab = TriStateImage::load(skin_dir, "C", "ico")
            .ok_or_else(|| "缺少 C_Normal.ico".to_string())?;
        let search_btn = TriStateImage::load(skin_dir, "SearchButton", "bmp");

        let group_bar = load_req("Bar.bmp")?;
        let scroll_thumb = RgbaSurface::load_file(&skin_dir.join("Scroll.bmp"));
        let up_down_arrows = RgbaSurface::load_file(&skin_dir.join("UpDown.bmp"));
        let menu_left_strip = load_req("MenuLeft.bmp")?;
        let menu_check = load_req("MenuCheck.ico")?;

        let mut tool_buttons = HashMap::new();
        for kind in ToolButtonKind::ALL {
            let (_, _, idx) = kind.config_keys();
            let prefix = format!("button{}", idx);
            if let Some(tri) = TriStateImage::load(skin_dir, &prefix, "ico") {
                tool_buttons.insert(kind, tri);
            }
        }

        let folder_name = skin_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let preview_thumbnail =
            RgbaSurface::load_file(&skin_dir.join(format!("{}.gif", folder_name)))
                .or_else(|| RgbaSurface::load_file(&skin_dir.join("Pre.gif")));

        Ok(Self {
            skin_left,
            skin_top,
            skin_right,
            skin_bottom,
            skin_top_left,
            skin_top_right,
            skin_bottom_left,
            skin_bottom_right,
            btn_min,
            btn_skin,
            btn_close,
            preview_base,
            category_tab,
            search_btn,
            group_bar,
            scroll_thumb,
            up_down_arrows,
            menu_left_strip,
            menu_check,
            tool_buttons,
            preview_thumbnail,
        })
    }
}

/// 内置与用户自定义图标资源库
#[derive(Debug, Clone)]
pub struct BuiltinIcons {
    pub folder_large: RgbaSurface,
    pub cat_laptop: RgbaSurface,
    pub cat_cube: RgbaSurface,
    pub group_search: RgbaSurface,
    pub menu_options: RgbaSurface,
    pub menu_hotkey: RgbaSurface,
    pub menu_schedule: RgbaSurface,
    pub menu_mail: RgbaSurface,
    pub menu_folder: RgbaSurface,
    pub menu_skin: RgbaSurface,
    pub menu_exit: RgbaSurface,
    pub ctx_add_file: RgbaSurface,
    pub ctx_add_dir: RgbaSurface,
    pub ctx_add_url: RgbaSurface,
    pub ctx_add_sys: RgbaSurface,
    pub ctx_add_login: RgbaSurface,
    pub ctx_search: RgbaSurface,
    pub ctx_appearance: RgbaSurface,
    pub ctx_paste: RgbaSurface,
    pub ctx_clear_col: RgbaSurface,
    pub ctx_lock_col: RgbaSurface,
    pub ctx_del_col: RgbaSurface,
    pub im_delete: RgbaSurface,
    pub im_cut: RgbaSurface,
    pub im_copy: RgbaSurface,
    pub im_schedule: RgbaSurface,
    pub im_move_top: RgbaSurface,
    pub im_move_bottom: RgbaSurface,
    pub im_edit: RgbaSurface,
    pub im_props: RgbaSurface,
    pub about_banner: RgbaSurface,
    pub app_icons: HashMap<String, RgbaSurface>,
}

impl BuiltinIcons {
    /// 加载 `assets/icons/` 与 `user/icons/` 下的所有像素图标
    pub fn load(base_dir: &Path) -> Self {
        let icon_dir = base_dir.join("assets").join("icons");
        let load_or_empty = |name: &str, w: u32, h: u32| -> RgbaSurface {
            RgbaSurface::load_file(&icon_dir.join(format!("{}.png", name)))
                .unwrap_or_else(|| RgbaSurface::new_empty(w, h))
        };
        let about_banner = RgbaSurface::load_file(&base_dir.join("assets").join("about_banner.png"))
            .unwrap_or_else(|| RgbaSurface::new_empty(137, 360));

        let mut app_icons = HashMap::new();
        for key in [
            "app_msedge",
            "app_7zfm",
            "app_erp",
            "app_everything",
            "app_yuque",
            "app_univpn",
            "app_wechat",
            "app_prevent",
            "app_clash",
            "app_vscode",
            "app_obsidian",
            "app_notepadpp",
            "app_hoppscotch",
            "app_pixpin",
            "app_tabby",
            "app_meatshell",
            "app_fiddler",
            "app_navicat",
            "app_myhash64",
            "app_typora",
            "app_proxylane64",
        ] {
            app_icons.insert(key.to_string(), load_or_empty(key, 16, 16));
        }

        // 加载用户拖入文件生成的自定义图标 (`user/icons/*.png`)
        let user_icon_dir = base_dir.join("user").join("icons");
        if let Ok(entries) = fs::read_dir(&user_icon_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().map(|e| e == "png").unwrap_or(false) {
                    if let Some(stem) = p.file_stem().map(|s| s.to_string_lossy().to_string()) {
                        if let Some(surf) = RgbaSurface::load_file(&p) {
                            app_icons.insert(stem, surf);
                        }
                    }
                }
            }
        }

        Self {
            folder_large: load_or_empty("folder_large", 32, 32),
            cat_laptop: load_or_empty("cat_laptop", 16, 16),
            cat_cube: load_or_empty("cat_cube", 16, 16),
            group_search: load_or_empty("group_search", 16, 16),
            menu_options: load_or_empty("menu_options", 16, 16),
            menu_hotkey: load_or_empty("menu_hotkey", 16, 16),
            menu_schedule: load_or_empty("menu_schedule", 16, 16),
            menu_mail: load_or_empty("menu_mail", 16, 16),
            menu_folder: load_or_empty("menu_folder", 16, 16),
            menu_skin: load_or_empty("menu_skin", 16, 16),
            menu_exit: load_or_empty("menu_exit", 16, 16),
            ctx_add_file: load_or_empty("ctx_add_file", 16, 16),
            ctx_add_dir: load_or_empty("ctx_add_dir", 16, 16),
            ctx_add_url: load_or_empty("ctx_add_url", 16, 16),
            ctx_add_sys: load_or_empty("ctx_add_sys", 16, 16),
            ctx_add_login: load_or_empty("ctx_add_login", 16, 16),
            ctx_search: load_or_empty("ctx_search", 16, 16),
            ctx_appearance: load_or_empty("ctx_appearance", 16, 16),
            ctx_paste: load_or_empty("ctx_paste", 16, 16),
            ctx_clear_col: load_or_empty("ctx_clear_col", 16, 16),
            ctx_lock_col: load_or_empty("ctx_lock_col", 16, 16),
            ctx_del_col: load_or_empty("ctx_del_col", 16, 16),
            im_delete: load_or_empty("im_delete", 16, 16),
            im_cut: load_or_empty("im_cut", 16, 16),
            im_copy: load_or_empty("im_copy", 16, 16),
            im_schedule: load_or_empty("im_schedule", 16, 16),
            im_move_top: load_or_empty("im_move_top", 16, 16),
            im_move_bottom: load_or_empty("im_move_bottom", 16, 16),
            im_edit: load_or_empty("im_edit", 16, 16),
            im_props: load_or_empty("im_props", 16, 16),
            about_banner,
            app_icons,
        }
    }
}

/// 32 位 BGRA 离屏像素画布
pub struct Canvas {
    pub width: i32,
    pub height: i32,
    pub buffer: Vec<u32>,
}

impl Canvas {
    pub fn new(width: i32, height: i32) -> Self {
        let size = (width.max(1) * height.max(1)) as usize;
        Self {
            width: width.max(1),
            height: height.max(1),
            buffer: vec![0; size],
        }
    }

    pub fn resize(&mut self, width: i32, height: i32) {
        let w = width.max(1);
        let h = height.max(1);
        if self.width != w || self.height != h {
            self.width = w;
            self.height = h;
            self.buffer.resize((w * h) as usize, 0);
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: RgbColor) {
        let x0 = x.clamp(0, self.width);
        let y0 = y.clamp(0, self.height);
        let x1 = (x + w).clamp(0, self.width);
        let y1 = (y + h).clamp(0, self.height);
        let pixel = color.to_bgra_u32();

        for py in y0..y1 {
            let row_start = (py * self.width + x0) as usize;
            let row_end = (py * self.width + x1) as usize;
            self.buffer[row_start..row_end].fill(pixel);
        }
    }

    pub fn fill_rect_alpha(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: RgbColor,
        alpha: u8,
    ) {
        let x0 = x.clamp(0, self.width);
        let y0 = y.clamp(0, self.height);
        let x1 = (x + w).clamp(0, self.width);
        let y1 = (y + h).clamp(0, self.height);

        for py in y0..y1 {
            for px in x0..x1 {
                self.blend_pixel(px, py, [color.r, color.g, color.b, alpha]);
            }
        }
    }

    pub fn stroke_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: RgbColor) {
        if w <= 0 || h <= 0 {
            return;
        }
        self.draw_hline(x, x + w - 1, y, color);
        self.draw_hline(x, x + w - 1, y + h - 1, color);
        self.draw_vline(x, y, y + h - 1, color);
        self.draw_vline(x + w - 1, y, y + h - 1, color);
    }

    /// 绘制 3D 凸起或凹陷经典边框（用于类别管理、栏目排序、热键管理、修改快捷方式等经典对话框按钮与输入框）
    pub fn stroke_3d_rect(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        top_left: RgbColor,
        bottom_right: RgbColor,
    ) {
        if w <= 0 || h <= 0 {
            return;
        }
        self.draw_hline(x, x + w - 1, y, top_left);
        self.draw_vline(x, y, y + h - 1, top_left);
        self.draw_hline(x, x + w - 1, y + h - 1, bottom_right);
        self.draw_vline(x + w - 1, y, y + h - 1, bottom_right);
    }

    pub fn draw_hline(&mut self, x0: i32, x1: i32, y: i32, color: RgbColor) {
        if y < 0 || y >= self.height {
            return;
        }
        let start = x0.min(x1).clamp(0, self.width - 1);
        let end = x0.max(x1).clamp(0, self.width - 1);
        let pixel = color.to_bgra_u32();
        for x in start..=end {
            self.buffer[(y * self.width + x) as usize] = pixel;
        }
    }

    pub fn draw_vline(&mut self, x: i32, y0: i32, y1: i32, color: RgbColor) {
        if x < 0 || x >= self.width {
            return;
        }
        let start = y0.min(y1).clamp(0, self.height - 1);
        let end = y0.max(y1).clamp(0, self.height - 1);
        let pixel = color.to_bgra_u32();
        for y in start..=end {
            self.buffer[(y * self.width + x) as usize] = pixel;
        }
    }

    #[inline]
    pub fn blend_pixel(&mut self, x: i32, y: i32, src: [u8; 4]) {
        if x < 0 || x >= self.width || y < 0 || y >= self.height {
            return;
        }
        let a = src[3] as u32;
        if a == 0 {
            return;
        }
        let idx = (y * self.width + x) as usize;
        if a == 255 {
            self.buffer[idx] =
                (src[2] as u32) | ((src[1] as u32) << 8) | ((src[0] as u32) << 16) | 0xFF00_0000;
        } else {
            let dst = self.buffer[idx];
            let db = dst & 0xFF;
            let dg = (dst >> 8) & 0xFF;
            let dr = (dst >> 16) & 0xFF;
            let inv_a = 255 - a;
            let r = (src[0] as u32 * a + dr * inv_a) / 255;
            let g = (src[1] as u32 * a + dg * inv_a) / 255;
            let b = (src[2] as u32 * a + db * inv_a) / 255;
            self.buffer[idx] = b | (g << 8) | (r << 16) | 0xFF00_0000;
        }
    }

    pub fn blit(&mut self, surf: &RgbaSurface, dst_x: i32, dst_y: i32) {
        self.blit_subrect(
            surf,
            0,
            0,
            surf.width,
            surf.height,
            dst_x,
            dst_y,
            surf.width as i32,
            surf.height as i32,
        );
    }

    pub fn blit_stretch(
        &mut self,
        surf: &RgbaSurface,
        dst_x: i32,
        dst_y: i32,
        dst_w: i32,
        dst_h: i32,
    ) {
        self.blit_subrect(
            surf,
            0,
            0,
            surf.width,
            surf.height,
            dst_x,
            dst_y,
            dst_w,
            dst_h,
        );
    }

    pub fn blit_subrect(
        &mut self,
        surf: &RgbaSurface,
        src_x: u32,
        src_y: u32,
        src_w: u32,
        src_h: u32,
        dst_x: i32,
        dst_y: i32,
        dst_w: i32,
        dst_h: i32,
    ) {
        if dst_w <= 0 || dst_h <= 0 || src_w == 0 || src_h == 0 {
            return;
        }
        for dy in 0..dst_h {
            let py = dst_y + dy;
            if py < 0 || py >= self.height {
                continue;
            }
            let sy = src_y + ((dy as u32 * src_h) / dst_h as u32).min(src_h - 1);
            for dx in 0..dst_w {
                let px = dst_x + dx;
                if px < 0 || px >= self.width {
                    continue;
                }
                let sx = src_x + ((dx as u32 * src_w) / dst_w as u32).min(src_w - 1);
                let p = surf.get_pixel(sx, sy);
                self.blend_pixel(px, py, p);
            }
        }
    }

    pub fn blit_horizontal_3slice(
        &mut self,
        surf: &RgbaSurface,
        src_y: u32,
        src_h: u32,
        border_w: u32,
        dst_x: i32,
        dst_y: i32,
        dst_w: i32,
        dst_h: i32,
    ) {
        if dst_w <= 0 || dst_h <= 0 || surf.width <= border_w * 2 {
            return;
        }
        let bw = border_w as i32;
        let mid_src_w = surf.width - border_w * 2;
        let mid_dst_w = (dst_w - bw * 2).max(0);

        self.blit_subrect(surf, 0, src_y, border_w, src_h, dst_x, dst_y, bw, dst_h);
        self.blit_subrect(
            surf,
            border_w,
            src_y,
            mid_src_w,
            src_h,
            dst_x + bw,
            dst_y,
            mid_dst_w,
            dst_h,
        );
        self.blit_subrect(
            surf,
            surf.width - border_w,
            src_y,
            border_w,
            src_h,
            dst_x + bw + mid_dst_w,
            dst_y,
            bw,
            dst_h,
        );
    }
}
