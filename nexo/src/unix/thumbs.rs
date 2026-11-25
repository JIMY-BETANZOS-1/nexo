use std::path::Path;
use std::process::Command;

pub use crate::bmp::jb_bmp;

pub fn jb_init_thread() {}

fn jb_decode(path: &Path, size: u32) -> Option<(usize, usize, Vec<u8>)> {
    let img = image::open(path).ok()?.thumbnail(size, size).to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut px = img.into_raw();
    for p in px.as_chunks_mut::<4>().0 {
        let a = p[3] as u16;
        for c in &mut p[..3] {
            *c = ((*c as u16 * a + 127) / 255) as u8;
        }
    }
    Some((w, h, px))
}

fn jb_external(path: &Path, size: u32) -> Option<(usize, usize, Vec<u8>)> {
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
        } else if crate::rules::jb_protection(path) == crate::rules::JbProtect::Video {
            let _ = Command::new("ffmpegthumbnailer").arg("-i").arg(path).arg("-o").arg(&out).args(["-s", &size.to_string()]).output();
        }
    }
    let r = jb_decode(&out, size);
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

pub fn jb_shell_thumbnail(path: &Path, size: u32) -> Option<(usize, usize, Vec<u8>)> {
    jb_decode(path, size).or_else(|| jb_external(path, size))
}
