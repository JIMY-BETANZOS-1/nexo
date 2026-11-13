//! Instalación ligera en el perfil del usuario (sin permisos de administrador):
//! copia estable del programa, acceso en el menú Inicio, nombre propio en las
//! notificaciones y arranque del vigía al iniciar sesión.

use std::path::PathBuf;
use std::ptr::null_mut;

use windows_sys::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

use crate::{icon, sys, vigia};

pub const AUMID: &str = "NEXO.Optimizador";
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "NEXO Vigia";

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn set_user_string(key: &str, name: &str, value: &str) -> bool {
    let data = w(value);
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            w(key).as_ptr(),
            w(name).as_ptr(),
            REG_SZ,
            data.as_ptr() as *const _,
            (data.len() * 2) as u32,
        ) == 0
    }
}

fn get_user_string(key: &str, name: &str) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut sz = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w(key).as_ptr(),
            w(name).as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut _,
            &mut sz,
        )
    };
    (r == 0).then(|| {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
        String::from_utf16_lossy(&buf[..len])
    })
}

pub fn install_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("Programs").join("NEXO")
}

pub fn installed_exe() -> PathBuf {
    install_dir().join("nexo.exe")
}

fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

fn outdated(src: &std::path::Path, dest: &std::path::Path) -> bool {
    match (std::fs::metadata(src), std::fs::metadata(dest)) {
        (Ok(s), Ok(d)) => s.len() != d.len() || s.modified().ok() > d.modified().ok(),
        _ => true,
    }
}

/// Hay una copia instalada más vieja que el NEXO que se está ejecutando.
pub fn needs_update() -> bool {
    let dest = installed_exe();
    match std::env::current_exe() {
        Ok(cur) => dest.exists() && !same_file(&cur, &dest) && outdated(&cur, &dest),
        Err(_) => false,
    }
}

/// Copia (o actualiza) NEXO a su carpeta fija y lo registra en Windows.
pub fn install() -> Result<PathBuf, String> {
    let dest = installed_exe();
    let cur = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = install_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    if !same_file(&cur, &dest) && outdated(&cur, &dest) {
        // El vigía usa la copia instalada: se detiene un momento para poder actualizarla.
        let was_running = vigia::stop_and_wait();
        let copied = std::fs::copy(&cur, &dest);
        if was_running {
            vigia::start();
        }
        if let Err(e) = copied {
            if !dest.exists() {
                return Err(format!("No se pudo copiar NEXO: {}", e));
            }
        }
    }

    // La app del celular viaja junto a NEXO para instalarla desde la pestaña Celulares.
    if let Some(apk) = cur.parent().map(|d| d.join("nexo-movil.apk")).filter(|p| p.exists()) {
        let _ = std::fs::copy(apk, dir.join("nexo-movil.apk"));
    }

    let ico = dir.join("nexo.ico");
    let _ = icon::write_ico(&ico);
    register_notifications(&ico);
    start_menu_shortcut(&dest, &ico);
    Ok(dest)
}

/// Las notificaciones salen como "NEXO" con su icono (y no como "Windows PowerShell").
fn register_notifications(ico: &std::path::Path) {
    let key = format!("Software\\Classes\\AppUserModelId\\{}", AUMID);
    set_user_string(&key, "DisplayName", "NEXO");
    set_user_string(&key, "IconUri", &ico.display().to_string());
    set_user_string(&key, "IconBackgroundColor", "FF05080F");
}

pub fn notifications_registered() -> bool {
    get_user_string(&format!("Software\\Classes\\AppUserModelId\\{}", AUMID), "DisplayName").is_some()
}

fn start_menu_shortcut(exe: &std::path::Path, ico: &std::path::Path) {
    let Some(appdata) = std::env::var_os("APPDATA") else { return };
    let lnk = PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs\\NEXO.lnk");
    if lnk.exists() {
        return;
    }
    let q = |p: &std::path::Path| p.display().to_string().replace('\'', "''");
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.IconLocation='{}';$s.Description='NEXO - limpieza y optimización local';$s.Save()",
        q(&lnk),
        q(exe),
        q(ico)
    );
    let _ = sys::hidden_cmd("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]).status();
}

fn shortcut_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Microsoft\\Windows\\Start Menu\\Programs\\NEXO.lnk"))
}

/// Deshace todo lo que NEXO registró en Windows. Conserva el historial y la configuración.
pub fn uninstall() -> Vec<String> {
    let mut done = Vec::new();
    if crate::schedule::is_registered() && crate::schedule::disable().is_ok() {
        done.push("tarea automática".to_string());
    }
    if vigia::stop_and_wait() {
        done.push("vigía detenido".into());
    }
    if set_autostart(false) {
        done.push("arranque con Windows".into());
    }
    if let Some(l) = shortcut_path().filter(|p| p.exists()) {
        if std::fs::remove_file(l).is_ok() {
            done.push("menú Inicio".into());
        }
    }
    let key = w(&format!("Software\\Classes\\AppUserModelId\\{}", AUMID));
    if unsafe { windows_sys::Win32::System::Registry::RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr()) } == 0 {
        done.push("notificaciones".into());
    }
    let dir = install_dir();
    if dir.exists() {
        let running_from_it = std::env::current_exe().map(|e| same_file(e.parent().unwrap_or(&e), &dir)).unwrap_or(false);
        if running_from_it {
            // No se puede borrar el programa en uso: se borra en cuanto se cierre.
            let cmd = format!("ping 127.0.0.1 -n 4 >nul & rmdir /s /q \"{}\"", dir.display());
            let _ = sys::hidden_cmd("cmd").args(["/c", &cmd]).spawn();
            done.push("archivos (al cerrar)".into());
        } else if std::fs::remove_dir_all(&dir).is_ok() {
            done.push("archivos".into());
        }
    }
    if done.is_empty() {
        done.push("no había nada instalado".into());
    }
    done
}

pub fn set_autostart(on: bool) -> bool {
    if on {
        let cmd = format!("\"{}\" --vigia", installed_exe().display());
        set_user_string(RUN_KEY, RUN_VALUE, &cmd)
    } else {
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, w(RUN_KEY).as_ptr(), w(RUN_VALUE).as_ptr()) == 0 }
    }
}

