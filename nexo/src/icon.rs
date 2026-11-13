//! Icono de NEXO (hexágono de neón) generado en código, para la ventana, la bandeja y Windows.

use std::f32::consts::{FRAC_PI_2, TAU};

/// Píxeles RGBA de n×n.
pub fn rgba(n: u32) -> Vec<u8> {
    let mut out = vec![0u8; (n * n * 4) as usize];
    let s = n as f32 / 64.0;
    let c = n as f32 / 2.0;
    for y in 0..n {
        for x in 0..n {
            let dx = x as f32 + 0.5 - c;
            let dy = y as f32 + 0.5 - c;
            let d = (dx * dx + dy * dy).sqrt();
            let ang = dy.atan2(dx);
            let sector = (ang + FRAC_PI_2).rem_euclid(TAU / 6.0) - TAU / 12.0;
            let hex = d * sector.cos() / (TAU / 12.0).cos();
            let ring = (1.0 - ((hex - 26.0 * s).abs() / (2.4 * s).max(1.0))).clamp(0.0, 1.0);
            let core = (1.0 - (d - 6.0 * s).max(0.0) / (1.5 * s).max(0.8)).clamp(0.0, 1.0);
            let mid = (1.0 - ((d - 13.0 * s).abs() / (1.7 * s).max(0.9))).clamp(0.0, 1.0);
            let i = ((y * n + x) * 4) as usize;
            let (r, g, b, a) = if ring > 0.0 || core > 0.0 {
                (0.0, 229.0, 255.0, ring.max(core))
            } else if mid > 0.0 {
                (139.0, 92.0, 246.0, mid)
            } else {
                (8.0, 14.0, 28.0, if hex < 26.0 * s { 1.0 } else { 0.0 })
            };
            out[i] = r as u8;
            out[i + 1] = g as u8;
            out[i + 2] = b as u8;
            out[i + 3] = (a * 255.0) as u8;
        }
    }
    out
}

/// Archivo .ico (64×64, 32 bits) para el acceso directo y las notificaciones.
#[cfg_attr(unix, allow(dead_code))]
pub fn write_ico(path: &std::path::Path) -> std::io::Result<()> {
    let n = 64u32;
    let px = rgba(n);
    let mask_row = (n.div_ceil(32) * 4) as usize;
    let img_size = 40 + (n * n * 4) as usize + mask_row * n as usize;
    let mut b: Vec<u8> = Vec::with_capacity(22 + img_size);
    b.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    b.extend_from_slice(&[64, 64, 0, 0, 1, 0, 32, 0]);
    b.extend_from_slice(&(img_size as u32).to_le_bytes());
    b.extend_from_slice(&22u32.to_le_bytes());
    // BITMAPINFOHEADER (alto doble: color + máscara)
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&(n as i32).to_le_bytes());
    b.extend_from_slice(&((n * 2) as i32).to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(&[0u8; 24]);
    for y in (0..n).rev() {
        for x in 0..n {
            let i = ((y * n + x) * 4) as usize;
            b.extend_from_slice(&[px[i + 2], px[i + 1], px[i], px[i + 3]]);
        }
    }
    b.extend(std::iter::repeat_n(0u8, mask_row * n as usize));
    std::fs::write(path, b)
}
