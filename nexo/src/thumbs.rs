//! Miniaturas de archivos con el mismo sistema que usa el Explorador de Windows.
//!
//! Sirve para fotos, videos, PDF y documentos (Windows ya las tiene en su caché).
//! Si un archivo no tiene miniatura, se devuelve su icono.

use std::ffi::c_void;
use std::mem::size_of;
use std::path::Path;

use windows::core::HSTRING;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_RESIZETOFIT};

/// Cada hilo que pide miniaturas necesita COM inicializado.
pub fn init_thread() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

/// (ancho, alto, RGBA premultiplicado) de la miniatura, del lado mayor `size`.
pub fn shell_thumbnail(path: &Path, size: u32) -> Option<(usize, usize, Vec<u8>)> {
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).ok()?;
        let hbmp = factory
            .GetImage(SIZE { cx: size as i32, cy: size as i32 }, SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK)
            .ok()?;
        let obj = HGDIOBJ(hbmp.0);
        let mut bm = BITMAP::default();
        GetObjectW(obj, size_of::<BITMAP>() as i32, Some(&mut bm as *mut BITMAP as *mut c_void));
        let (w, h) = (bm.bmWidth.max(0), bm.bmHeight.max(0));
        let mut out = None;
        if w > 0 && h > 0 {
            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // de arriba hacia abajo
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut buf = vec![0u8; (w * h * 4) as usize];
            let dc = GetDC(None);
            let lines = GetDIBits(dc, hbmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut c_void), &mut bi, DIB_RGB_COLORS);
            ReleaseDC(None, dc);
            if lines > 0 {
                // BGRA → RGBA. Las fotos vienen sin canal alfa: se vuelven opacas.
                let no_alpha = buf.as_chunks::<4>().0.iter().all(|p| p[3] == 0);
                for p in buf.as_chunks_mut::<4>().0 {
                    p.swap(0, 2);
                    if no_alpha {
                        p[3] = 255;
                    }
                }
                out = Some((w as usize, h as usize, buf));
            }
        }
        let _ = DeleteObject(obj);
        out
    }
}

pub use crate::bmp::bmp;
