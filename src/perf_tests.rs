//! 可选微基准；只统计内存图像操作，不启动程序或写入用户数据。
use crate::skin_assets::{Canvas, RgbaSurface};
use std::{hint::black_box, time::Instant};

#[test]
fn image_operation_benchmarks() {
    if std::env::var_os("KSTART_PERF").is_none() {
        return;
    }
    let image = RgbaSurface::new_empty(256, 256);
    let start = Instant::now();
    for _ in 0..2000 {
        black_box(image.clone());
    }
    let clones = start.elapsed();
    let image = RgbaSurface::new_empty(64, 64);
    let start = Instant::now();
    for _ in 0..300 {
        black_box(image.resize_to(16, 16));
    }
    let resizing = start.elapsed();
    let mut canvas = Canvas::new(300, 300);
    let start = Instant::now();
    for _ in 0..1500 {
        canvas.blit(black_box(&image), 20, 20);
    }
    let blitting = start.elapsed();
    println!(
        "图像基准：2000 次复制 {:?}，300 次缩放 {:?}，1500 次绘制 {:?}",
        clones, resizing, blitting
    );
}
