//! BGRA 画布和纯软件栅格绘制，不依赖窗口状态。
use crate::{skin_parser::RgbColor, surface::RgbaSurface};

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

    pub fn fill_rect_alpha(&mut self, x: i32, y: i32, w: i32, h: i32, color: RgbColor, alpha: u8) {
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
        let x0 = dst_x.max(0);
        let y0 = dst_y.max(0);
        let x1 = (dst_x + surf.width as i32).min(self.width);
        let y1 = (dst_y + surf.height as i32).min(self.height);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let width = (x1 - x0) as usize;
        let pixels = surf.pixels();
        for y in y0..y1 {
            let start = ((y - dst_y) as usize * surf.width as usize) + (x0 - dst_x) as usize;
            let destination = y as usize * self.width as usize + x0 as usize;
            let target = &mut self.buffer[destination..destination + width];
            for (dst, &[r, g, b, alpha]) in target.iter_mut().zip(&pixels[start..start + width]) {
                match alpha {
                    0 => {}
                    255 => *dst = u32::from_le_bytes([b, g, r, 255]),
                    _ => {
                        let a = u32::from(alpha);
                        let inverse = 255 - a;
                        let red = (u32::from(r) * a + ((*dst >> 16) & 255) * inverse) / 255;
                        let green = (u32::from(g) * a + ((*dst >> 8) & 255) * inverse) / 255;
                        let blue = (u32::from(b) * a + (*dst & 255) * inverse) / 255;
                        *dst = blue | (green << 8) | (red << 16) | 0xff00_0000;
                    }
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_blit_matches_general_blit_for_clipping_and_every_alpha() {
        let pixels = (0..256)
            .map(|alpha| [alpha as u8, 80, 173, alpha as u8])
            .collect();
        let surface = RgbaSurface::from_pixels(16, 16, pixels);
        for (x, y) in [(-20, 0), (0, -20), (-5, -7), (0, 0), (9, 13), (25, 25)] {
            let mut reference = Canvas::new(24, 24);
            reference.fill_rect(0, 0, 24, 24, RgbColor::new(25, 65, 95));
            let mut optimized = Canvas::new(24, 24);
            optimized.buffer.clone_from(&reference.buffer);
            reference.blit_subrect(&surface, 0, 0, 16, 16, x, y, 16, 16);
            optimized.blit(&surface, x, y);
            assert_eq!(optimized.buffer, reference.buffer, "位置 ({x}, {y})");
        }
    }
}
