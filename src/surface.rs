//! 共享图像表面、解码与缩放。
use crate::skin_parser::RgbColor;
use std::{
    fs,
    path::Path,
    sync::{Arc, OnceLock},
};

#[derive(Debug)]
struct ImageData {
    pixels: Vec<[u8; 4]>,
    small: OnceLock<Arc<ImageData>>,
    large: OnceLock<Arc<ImageData>>,
}

impl Clone for ImageData {
    fn clone(&self) -> Self {
        Self {
            pixels: self.pixels.clone(),
            small: OnceLock::new(),
            large: OnceLock::new(),
        }
    }
}

/// RGBA 图像表面
#[derive(Debug, Clone)]
pub struct RgbaSurface {
    pub width: u32,
    pub height: u32,
    data: Arc<ImageData>,
}

impl RgbaSurface {
    /// 创建指定尺寸的空白透明表面
    pub fn new_empty(width: u32, height: u32) -> Self {
        Self::from_pixels(
            width,
            height,
            vec![[0; 4]; width as usize * height as usize],
        )
    }

    pub fn from_pixels(width: u32, height: u32, pixels: Vec<[u8; 4]>) -> Self {
        assert_eq!(pixels.len(), width as usize * height as usize);
        Self {
            width,
            height,
            data: Arc::new(ImageData {
                pixels,
                small: OnceLock::new(),
                large: OnceLock::new(),
            }),
        }
    }

    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.data.pixels
    }

    /// 写时复制：修改一份图像不会污染其他使用者，并清除旧缩放缓存。
    pub fn pixels_mut(&mut self) -> &mut [[u8; 4]] {
        let data = Arc::make_mut(&mut self.data);
        data.small.take();
        data.large.take();
        &mut data.pixels
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
        Some(Self::from_pixels(width, height, pixels))
    }

    /// 将当前 RGBA 表面保存为 PNG 文件
    pub fn save_png(&self, path: &Path) -> bool {
        image::save_buffer_with_format(
            path,
            self.pixels().as_flattened(),
            self.width,
            self.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .is_ok()
    }

    /// 常用图标尺寸只缩放一次，所有副本共用结果；其他尺寸按需计算。
    pub fn resize_to(&self, target_w: u32, target_h: u32) -> Self {
        if self.width == target_w && self.height == target_h {
            return self.clone();
        }
        let cache = match (target_w, target_h) {
            (16, 16) => Some(&self.data.small),
            (32, 32) => Some(&self.data.large),
            _ => None,
        };
        if let Some(cache) = cache {
            let data = cache
                .get_or_init(|| self.resample(target_w, target_h).data)
                .clone();
            return Self {
                width: target_w,
                height: target_h,
                data,
            };
        }
        self.resample(target_w, target_h)
    }

    fn resample(&self, target_w: u32, target_h: u32) -> Self {
        let view = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
            self.width,
            self.height,
            self.pixels().as_flattened(),
        );
        if let Some(buf) = view {
            let resized = image::imageops::resize(
                &buf,
                target_w,
                target_h,
                image::imageops::FilterType::Lanczos3,
            );
            let pixels = resized.pixels().map(|p| p.0).collect();
            return Self::from_pixels(target_w, target_h, pixels);
        }
        Self::new_empty(target_w, target_h)
    }

    /// 获取 `(x, y)` 处的像素
    #[inline]
    pub fn get_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x < self.width && y < self.height {
            self.pixels()[(y * self.width + x) as usize]
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
        let width = self.width;
        let pixels = self.pixels_mut();
        for y in ry..y_end {
            for x in rx..x_end {
                let idx = (y * width + x) as usize;
                let p = pixels[idx];
                if p[0] == mask_color.r && p[1] == mask_color.g && p[2] == mask_color.b {
                    pixels[idx][3] = 0;
                }
            }
        }
    }
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
        return Some(RgbaSurface::from_pixels(w, h, pixels));
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

    Some(RgbaSurface::from_pixels(width, height, pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_pixels_but_mutations_are_isolated() {
        let image = RgbaSurface::from_pixels(1, 1, vec![[12, 34, 56, 255]]);
        let mut copy = image.clone();
        assert_eq!(image.pixels().as_ptr(), copy.pixels().as_ptr());
        copy.pixels_mut()[0] = [0; 4];
        assert_eq!(image.get_pixel(0, 0), [12, 34, 56, 255]);
        assert_eq!(copy.get_pixel(0, 0), [0; 4]);
    }

    #[test]
    fn icon_sizes_share_cached_results_and_changes_invalidate_them() {
        let mut image = RgbaSurface::from_pixels(64, 64, vec![[45, 90, 135, 255]; 64 * 64]);
        let small = image.resize_to(16, 16);
        let copy = image.clone();
        assert_eq!(
            small.pixels().as_ptr(),
            copy.resize_to(16, 16).pixels().as_ptr()
        );
        let expected = image.resample(16, 16);
        assert_eq!(small.pixels(), expected.pixels());
        image.pixels_mut().fill([0; 4]);
        assert_eq!(image.resize_to(16, 16).get_pixel(0, 0), [0; 4]);
        assert_eq!(copy.resize_to(16, 16).pixels(), small.pixels());
    }
}
