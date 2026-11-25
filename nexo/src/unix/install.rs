use std::path::{Path, PathBuf};

use crate::{icon, vigia};

fn jb_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

pub fn jb_install_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    return jb_home().join("Library/Application Support/NEXO/bin");
    #[cfg(not(target_os = "macos"))]
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| jb_home().join(".local/share")).join("nexo")
}

fn jb_packaged() -> Option<PathBuf> {
    if let Some(a) = std::env::var_os("APPIMAGE") {
        return Some(PathBuf::from(a));
    }
    let cur = std::env::current_exe().ok()?.canonicalize().ok()?;
    let s = cur.to_string_lossy();
    let fixed = s.starts_with("/usr/") || s.starts_with("/opt/") || (s.starts_with("/Applications/") && s.contains(".app/Contents/MacOS/"));
    fixed.then_some(cur)
}

pub fn jb_installed_exe() -> PathBuf {
    jb_packaged().unwrap_or_else(|| jb_install_dir().join("nexo"))
}

fn jb_same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn jb_outdated(src: &Path, dest: &Path) -> bool {
    match (std::fs::metadata(src), std::fs::metadata(dest)) {
        (Ok(s), Ok(d)) => s.len() != d.len() || s.modified().ok() > d.modified().ok(),
        _ => true,
    }
}

pub fn jb_needs_update() -> bool {
    if jb_packaged().is_some() {
        return false;
    }
    let dest = jb_installed_exe();
    match std::env::current_exe() {
        Ok(cur) => dest.exists() && !jb_same_file(&cur, &dest) && jb_outdated(&cur, &dest),
        Err(_) => false,
    }
}

fn jb_make_executable(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755));
}

pub fn jb_install() -> Result<PathBuf, String> {
    std::fs::create_dir_all(jb_install_dir()).map_err(|e| e.to_string())?;
    if let Some(p) = jb_packaged() {
        if std::env::var_os("APPIMAGE").is_some() {
            let png = jb_install_dir().join("nexo.png");
            let _ = image::save_buffer(&png, &icon::jb_rgba(128), 128, 128, image::ExtendedColorType::Rgba8);
            jb_shortcut(&p, &png);
        }
        return Ok(p);
    }
    let dest = jb_installed_exe();
    let cur = std::env::current_exe().map_err(|e| e.to_string())?;
    if !jb_same_file(&cur, &dest) && jb_outdated(&cur, &dest) {
        let was_running = vigia::jb_stop_and_wait();
        let tmp = dest.with_extension("nuevo");
        let copied = std::fs::copy(&cur, &tmp).and_then(|_| std::fs::rename(&tmp, &dest));
        jb_make_executable(&dest);
        if was_running {
            vigia::jb_start();
        }
        if let Err(e) = copied {
            if !dest.exists() {
                return Err(format!("No se pudo copiar NEXO: {}", e));
            }
        }
    }
    let png = jb_install_dir().join("nexo.png");
    let px = icon::jb_rgba(128);
    let _ = image::save_buffer(&png, &px, 128, 128, image::ExtendedColorType::Rgba8);
    jb_shortcut(&dest, &png);
    Ok(dest)
}

#[cfg(not(target_os = "macos"))]
fn jb_desktop_file() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| jb_home().join(".local/share")).join("applications/nexo.desktop")
}

#[cfg(not(target_os = "macos"))]
fn jb_shortcut(exe: &Path, png: &Path) {
    let f = jb_desktop_file();
    let _ = std::fs::create_dir_all(f.parent().unwrap_or(&f));
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=NEXO\nComment=Limpieza segura, memoria, diagnóstico y protección (100 % local)\nExec=\"{}\"\nIcon={}\nTerminal=false\nCategories=System;Utility;\n",
        exe.display(),
        png.display()
    );
    let _ = std::fs::write(f, text);
}

#[cfg(target_os = "macos")]
fn jb_app_bundle() -> PathBuf {
    jb_home().join("Applications/NEXO.app")
}

#[cfg(target_os = "macos")]
fn jb_shortcut(exe: &Path, png: &Path) {
    let app = jb_app_bundle();
    let macos = app.join("Contents/MacOS");
    let res = app.join("Contents/Resources");
    let _ = std::fs::create_dir_all(&macos);
    let _ = std::fs::create_dir_all(&res);
    let _ = std::fs::copy(png, res.join("nexo.png"));
    let launcher = macos.join("NEXO");
    let _ = std::fs::write(&launcher, format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", exe.display()));
    jb_make_executable(&launcher);
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

#[allow(dead_code)]
pub fn jb_notifications_registered() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
fn jb_autostart_file() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| jb_home().join(".config")).join("autostart/nexo-vigia.desktop")
}

#[cfg(target_os = "macos")]
fn jb_autostart_file() -> PathBuf {
    jb_home().join("Library/LaunchAgents/com.nexo.vigia.plist")
}

pub fn jb_set_autostart(on: bool) -> bool {
    let f = jb_autostart_file();
    if !on {
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("launchctl").arg("unload").arg(&f).output();
        return std::fs::remove_file(&f).is_ok();
    }
    let _ = std::fs::create_dir_all(f.parent().unwrap_or(&f));
    let exe = jb_installed_exe();
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

pub fn jb_uninstall() -> Vec<String> {
    let mut done = Vec::new();
    if crate::schedule::jb_is_registered() && crate::schedule::jb_disable().is_ok() {
        done.push("tarea automática".to_string());
    }
    if vigia::jb_stop_and_wait() {
        done.push("vigía detenido".into());
    }
    if jb_set_autostart(false) {
        done.push("arranque al iniciar sesión".into());
    }
    #[cfg(not(target_os = "macos"))]
    if std::fs::remove_file(jb_desktop_file()).is_ok() {
        done.push("menú de aplicaciones".into());
    }
    #[cfg(target_os = "macos")]
    if std::fs::remove_dir_all(jb_app_bundle()).is_ok() {
        done.push("NEXO.app".into());
    }
    if jb_install_dir().exists() && std::fs::remove_dir_all(jb_install_dir()).is_ok() {
        done.push("archivos".into());
    }
    if done.is_empty() {
        done.push("no había nada instalado".into());
    }
    done
}
