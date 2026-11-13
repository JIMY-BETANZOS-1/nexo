//! Instalación ligera en el perfil del usuario (sin contraseña) en Linux y macOS.
//!
//! Linux: copia en ~/.local/share/nexo, acceso en el menú de aplicaciones (archivo .desktop)
//!        y arranque del vigía en ~/.config/autostart.
//! macOS: copia en ~/Library/Application Support/NEXO, aplicación NEXO.app en
//!        ~/Applications y arranque del vigía con un LaunchAgent.

use std::path::{Path, PathBuf};

use crate::{icon, vigia};

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

pub fn install_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    return home().join("Library/Application Support/NEXO/bin");
    #[cfg(not(target_os = "macos"))]
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share")).join("nexo")
}

/// NEXO instalado por un paquete del sistema (.deb, AppImage o NEXO.app en /Applications):
/// en ese caso el ejecutable ya tiene su sitio fijo y no se copia al perfil.
fn packaged() -> Option<PathBuf> {
    if let Some(a) = std::env::var_os("APPIMAGE") {
        return Some(PathBuf::from(a));
    }
    let cur = std::env::current_exe().ok()?.canonicalize().ok()?;
    let s = cur.to_string_lossy();
    let fixed = s.starts_with("/usr/") || s.starts_with("/opt/") || (s.starts_with("/Applications/") && s.contains(".app/Contents/MacOS/"));
    fixed.then_some(cur)
}

pub fn installed_exe() -> PathBuf {
    packaged().unwrap_or_else(|| install_dir().join("nexo"))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn outdated(src: &Path, dest: &Path) -> bool {
    match (std::fs::metadata(src), std::fs::metadata(dest)) {
        (Ok(s), Ok(d)) => s.len() != d.len() || s.modified().ok() > d.modified().ok(),
        _ => true,
    }
}

/// Hay una copia instalada más vieja que el NEXO que se está ejecutando.
pub fn needs_update() -> bool {
    if packaged().is_some() {
        return false;
    }
    let dest = installed_exe();
    match std::env::current_exe() {
        Ok(cur) => dest.exists() && !same_file(&cur, &dest) && outdated(&cur, &dest),
        Err(_) => false,
    }
}

fn make_executable(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755));
}

/// Copia (o actualiza) NEXO a su carpeta fija y crea el acceso directo.
pub fn install() -> Result<PathBuf, String> {
    std::fs::create_dir_all(install_dir()).map_err(|e| e.to_string())?;
    if let Some(p) = packaged() {
        // El paquete .deb y NEXO.app ya traen su acceso; la AppImage no, así que se le crea uno.
        if std::env::var_os("APPIMAGE").is_some() {
            let png = install_dir().join("nexo.png");
            let _ = image::save_buffer(&png, &icon::rgba(128), 128, 128, image::ExtendedColorType::Rgba8);
            shortcut(&p, &png);
        }
        return Ok(p);
    }
    let dest = installed_exe();
    let cur = std::env::current_exe().map_err(|e| e.to_string())?;
    if !same_file(&cur, &dest) && outdated(&cur, &dest) {
        let was_running = vigia::stop_and_wait();
        // Se copia a un archivo nuevo y se renombra: así no se pisa un ejecutable en uso.
        let tmp = dest.with_extension("nuevo");
        let copied = std::fs::copy(&cur, &tmp).and_then(|_| std::fs::rename(&tmp, &dest));
        make_executable(&dest);
        if was_running {
            vigia::start();
        }
        if let Err(e) = copied {
            if !dest.exists() {
                return Err(format!("No se pudo copiar NEXO: {}", e));
            }
        }
    }
    let png = install_dir().join("nexo.png");
    let px = icon::rgba(128);
    let _ = image::save_buffer(&png, &px, 128, 128, image::ExtendedColorType::Rgba8);
    shortcut(&dest, &png);
    Ok(dest)
}

#[cfg(not(target_os = "macos"))]
fn desktop_file() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share")).join("applications/nexo.desktop")
}

#[cfg(not(target_os = "macos"))]
fn shortcut(exe: &Path, png: &Path) {
    let f = desktop_file();
    let _ = std::fs::create_dir_all(f.parent().unwrap_or(&f));
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=NEXO\nComment=Limpieza segura, memoria, diagnóstico y protección (100 % local)\nExec=\"{}\"\nIcon={}\nTerminal=false\nCategories=System;Utility;\n",
        exe.display(),
        png.display()
    );
    let _ = std::fs::write(f, text);
}

#[cfg(target_os = "macos")]
fn app_bundle() -> PathBuf {
    home().join("Applications/NEXO.app")
}

/// Crea ~/Applications/NEXO.app: aparece en Launchpad y en Spotlight.
#[cfg(target_os = "macos")]
fn shortcut(exe: &Path, png: &Path) {
    let app = app_bundle();
    let macos = app.join("Contents/MacOS");
    let res = app.join("Contents/Resources");
    let _ = std::fs::create_dir_all(&macos);
    let _ = std::fs::create_dir_all(&res);
    let _ = std::fs::copy(png, res.join("nexo.png"));
    // El lanzador abre la copia instalada: así actualizarla no rompe la app.
    let launcher = macos.join("NEXO");
    let _ = std::fs::write(&launcher, format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", exe.display()));
    make_executable(&launcher);
    let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>NEXO</string>
<key>CFBundleDisplayName</key><string>NEXO</string>
<key>CFBundleIdentifier</key><string>com.nexo.optimizador</string>
<key>CFBundleExecutable</key><string>NEXO</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>{}</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
"#;
    let _ = std::fs::write(app.join("Contents/Info.plist"), plist.replace("{}", env!("CARGO_PKG_VERSION")));
}

/// En Linux y macOS las notificaciones no necesitan registro previo.
#[allow(dead_code)]
pub fn notifications_registered() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
fn autostart_file() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("autostart/nexo-vigia.desktop")
}

#[cfg(target_os = "macos")]
fn autostart_file() -> PathBuf {
    home().join("Library/LaunchAgents/com.nexo.vigia.plist")
}

/// Arranca (o no) el vigía al iniciar sesión.
pub fn set_autostart(on: bool) -> bool {
    let f = autostart_file();
    if !on {
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("launchctl").arg("unload").arg(&f).output();
        return std::fs::remove_file(&f).is_ok();
    }
    let _ = std::fs::create_dir_all(f.parent().unwrap_or(&f));
    let exe = installed_exe();
    #[cfg(not(target_os = "macos"))]
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=NEXO Vigia\nExec=\"{}\" --vigia\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n",
        exe.display()
    );
    #[cfg(target_os = "macos")]
    let text = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>com.nexo.vigia</string>
<key>ProgramArguments</key><array><string>{}</string><string>--vigia</string></array>
<key>RunAtLoad</key><true/>
<key>ProcessType</key><string>Background</string>
</dict></plist>
"#,
        exe.display()
    );
    std::fs::write(&f, text).is_ok()
}

/// Deshace todo lo que NEXO registró en el sistema. Conserva el historial y la configuración.
pub fn uninstall() -> Vec<String> {
    let mut done = Vec::new();
    if crate::schedule::is_registered() && crate::schedule::disable().is_ok() {
        done.push("tarea automática".to_string());
    }
    if vigia::stop_and_wait() {
        done.push("vigía detenido".into());
    }
    if set_autostart(false) {
        done.push("arranque al iniciar sesión".into());
    }
    #[cfg(not(target_os = "macos"))]
    if std::fs::remove_file(desktop_file()).is_ok() {
        done.push("menú de aplicaciones".into());
    }
    #[cfg(target_os = "macos")]
    if std::fs::remove_dir_all(app_bundle()).is_ok() {
        done.push("NEXO.app".into());
    }
    // En Linux y macOS se puede borrar un ejecutable en uso: el sistema lo libera al cerrarse.
    if install_dir().exists() && std::fs::remove_dir_all(install_dir()).is_ok() {
        done.push("archivos".into());
    }
    if done.is_empty() {
        done.push("no había nada instalado".into());
    }
    done
}
