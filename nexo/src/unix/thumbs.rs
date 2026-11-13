//! Miniaturas en Linux y macOS.
//!
//! Las imágenes (PNG, JPG, GIF, BMP, WebP) las decodifica NEXO. Para PDF y videos se usa
//! la herramienta del sistema si existe: Vista rápida (`qlmanage`) en macOS, y
//! `pdftoppm` o `ffmpegthumbnailer` en Linux. Si no hay miniatura, el visor muestra un icono.

use std::path::Path;
use std::process::Command;

pub use crate::bmp::bmp;

/// Linux y macOS no necesitan preparar el hilo (en Windows se inicializa COM).
pub fn init_thread() {}

fn decode(path: &Path, size: u32) -> Option<(usize, usize, Vec<u8>)> {
    let img = image::open(path).ok()?.thumbnail(size, size).to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut px = img.into_raw();
    // El visor espera RGBA premultiplicado (como devuelve Windows).
    for p in px.as_chunks_mut::<4>().0 {
        let a = p[3] as u16;
        for c in &mut p[..3] {
            *c = ((*c as u16 * a + 127) / 255) as u8;
        }
    }
    Some((w, h, px))
}

/// Genera la miniatura con una herramienta del sistema en una carpeta temporal.
fn external(path: &Path, size: u32) -> Option<(usize, usize, Vec<u8>)> {
    let tmp = std::env::temp_dir().join(format!("nexo_mini_{}_{}", std::process::id(), size));
    let _ = std::fs::create_dir_all(&tmp);
    let out = tmp.join("mini.png");
    let _ = std::fs::remove_file(&out);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    #[cfg(target_os = "macos")]
    {
        let _ = ext;
        let ok = Command::new("qlmanage").args(["-t", "-s", &size.to_string(), "-o"]).arg(&tmp).arg(path).output().map(|o| o.status.success()).unwrap_or(false);
        let name = format!("{}.png", path.file_name()?.to_string_lossy());
        if ok {
            let _ = std::fs::rename(tmp.join(name), &out);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        if ext == "pdf" {
            let _ = Command::new("pdftoppm")
                .args(["-png", "-singlefile", "-scale-to", &size.to_string()])
                .arg(path)
                .arg(tmp.join("mini"))
                .output();
        } else if crate::rules::protection(path) == crate::rules::Protect::Video {
            let _ = Command::new("ffmpegthumbnailer").arg("-i").arg(path).arg("-o").arg(&out).args(["-s", &size.to_string()]).output();
        }
    }
    let r = decode(&out, size);
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

/// (ancho, alto, RGBA premultiplicado) de la miniatura, del lado mayor `size`.
pub fn shell_thumbnail(path: &Path, size: u32) -> Option<(usize, usize, Vec<u8>)> {
    decode(path, size).or_else(|| external(path, size))
}
