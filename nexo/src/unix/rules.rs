//! Reglas de Linux y macOS: carpetas de basura conocidas y carpetas personales prohibidas.
//!
//! Las mismas clases y garantías que en Windows: la basura segura se borra, lo del usuario
//! va a la Papelera y los videos y documentos nunca se borran solos. Los identificadores
//! (temp_user, browsers, apps…) coinciden con los de Windows para que la configuración del
//! modo automático sirva en cualquier sistema.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// Carpeta del usuario según ~/.config/user-dirs.dirs (Linux; en español suele ser
/// «Documentos», «Descargas»…). Si no está definida, se usa el nombre en inglés.
#[cfg(not(target_os = "macos"))]
fn xdg_dir(key: &str, default: &str) -> PathBuf {
    let h = home();
    if let Ok(text) = std::fs::read_to_string(h.join(".config/user-dirs.dirs")) {
        for l in text.lines() {
            if let Some(v) = l.strip_prefix(&format!("XDG_{}_DIR=", key)) {
                let v = v.trim().trim_matches('"').replace("$HOME", &h.to_string_lossy());
                return PathBuf::from(v);
            }
        }
    }
    h.join(default)
}

pub fn detect() -> Env {
    let h = home();
    #[cfg(target_os = "macos")]
    let (docs, desk, vids, music, pics, down) = (h.join("Documents"), h.join("Desktop"), h.join("Movies"), h.join("Music"), h.join("Pictures"), h.join("Downloads"));
    #[cfg(not(target_os = "macos"))]
    let (docs, desk, vids, music, pics, down) = (
        xdg_dir("DOCUMENTS", "Documents"),
        xdg_dir("DESKTOP", "Desktop"),
        xdg_dir("VIDEOS", "Videos"),
        xdg_dir("MUSIC", "Music"),
        xdg_dir("PICTURES", "Pictures"),
        xdg_dir("DOWNLOAD", "Downloads"),
    );

    #[cfg(target_os = "macos")]
    let screenshots = Some(desk.clone()); // macOS guarda las capturas en el Escritorio
    #[cfg(not(target_os = "macos"))]
    let screenshots = ["Screenshots", "Capturas de pantalla", "Capturas"].iter().map(|n| pics.join(n)).find(|p| p.is_dir()).or_else(|| Some(pics.join("Screenshots")));

    let mut forbidden = vec![docs, desk, vids, music, pics, down.clone()];
    for name in ["Documents", "Desktop", "Videos", "Movies", "Music", "Pictures", "Downloads", "Documentos", "Escritorio", "Vídeos", "Música", "Imágenes", "Descargas", "Public", "Público", "Dropbox", "OneDrive", "Google Drive"] {
        forbidden.push(h.join(name));
    }
    #[cfg(target_os = "macos")]
    forbidden.push(h.join("Library/Mobile Documents")); // iCloud Drive

    #[cfg(target_os = "macos")]
    let (local, roaming) = (h.join("Library/Caches"), h.join("Library/Application Support"));
    #[cfg(not(target_os = "macos"))]
    let (local, roaming) = (
        std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|| h.join(".cache")),
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| h.join(".config")),
    );

    Env {
        local,
        roaming,
        temp: std::env::temp_dir(),
        windir: PathBuf::from("/"),
        programdata: PathBuf::from("/var"),
        downloads: Some(down),
        screenshots,
        forbidden,
    }
}

fn dirs(list: Vec<PathBuf>) -> Vec<PathBuf> {
    list.into_iter().filter(|p| p.is_dir()).collect()
}

/// Subcarpetas `Cache`, `Code Cache` y `GPUCache` de los perfiles de un navegador Chromium.
fn chromium_caches(base: &Path, procs: &'static [&'static str]) -> Vec<Target> {
    let mut out = Vec::new();
    let mut profiles = vec![base.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(base) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n == "Default" || n.starts_with("Profile ") || n == "Guest Profile" {
                profiles.push(e.path());
            }
        }
    }
    for p in profiles {
        for c in ["Cache", "Code Cache", "GPUCache", "Service Worker/CacheStorage"] {
            let d = p.join(c);
            if d.is_dir() {
                out.push(blocked(t(d, true, Filter::All, 0), procs));
            }
        }
    }
    out
}

fn firefox_caches(profiles_dir: &Path) -> Vec<Target> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(profiles_dir) {
        for e in rd.flatten() {
            for c in ["cache2", "startupCache", "thumbnails"] {
                let d = e.path().join(c);
                if d.is_dir() {
                    out.push(blocked(t(d, true, Filter::All, 0), &["firefox", "firefox-bin"]));
                }
            }
        }
    }
    out
}

fn browser_targets(env: &Env) -> Vec<Target> {
    let c = &env.local;
    let mut out = Vec::new();
    #[cfg(target_os = "macos")]
    {
        out.extend(chromium_caches(&c.join("Google/Chrome"), &["google chrome", "google chrome helper"]));
        out.extend(chromium_caches(&c.join("Microsoft Edge"), &["microsoft edge"]));
        out.extend(chromium_caches(&c.join("BraveSoftware/Brave-Browser"), &["brave browser"]));
        out.extend(firefox_caches(&c.join("Firefox/Profiles")));
    }
    #[cfg(not(target_os = "macos"))]
    {
        out.extend(chromium_caches(&c.join("google-chrome"), &["chrome", "google-chrome"]));
        out.extend(chromium_caches(&c.join("chromium"), &["chromium", "chromium-browser"]));
        out.extend(chromium_caches(&c.join("microsoft-edge"), &["msedge", "microsoft-edge"]));
        out.extend(chromium_caches(&c.join("BraveSoftware/Brave-Browser"), &["brave", "brave-browser"]));
        out.extend(firefox_caches(&c.join("mozilla/firefox")));
        out.extend(firefox_caches(&home().join(".mozilla/firefox")));
    }
    out
}

/// Cada carpeta de ~/Library/Caches por separado, sin las de los navegadores (tienen su
/// propia categoría) ni las de Apple (las gestiona macOS).
#[cfg(target_os = "macos")]
fn mac_app_caches(env: &Env) -> Vec<Target> {
    const SKIP: &[&str] = &["Google", "Firefox", "Microsoft Edge", "BraveSoftware", "com.google.Chrome", "org.mozilla.firefox"];
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&env.local) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if e.path().is_dir() && !n.starts_with("com.apple.") && !SKIP.contains(&n.as_str()) {
                out.push(t(e.path(), true, Filter::All, 24));
            }
        }
    }
    out
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn app_targets(env: &Env) -> Vec<Target> {
    let (c, r) = (&env.local, &env.roaming);
    let mut list: Vec<(PathBuf, &'static [&'static str])> = vec![
        (r.join("discord/Cache"), &["discord"]),
        (r.join("discord/Code Cache"), &["discord"]),
        (r.join("Code/Cache"), &["code", "code helper"]),
        (r.join("Code/CachedData"), &["code", "code helper"]),
        (r.join("Code/Code Cache"), &["code", "code helper"]),
        (r.join("Slack/Cache"), &["slack"]),
        (r.join("Microsoft/Teams/Cache"), &["teams"]),
    ];
    #[cfg(target_os = "macos")]
    list.push((c.join("com.spotify.client"), &["spotify"]));
    #[cfg(not(target_os = "macos"))]
    {
        list.push((c.join("spotify"), &["spotify"]));
        list.push((c.join("thumbnails"), &[]));
    }
    list.into_iter().filter(|(p, _)| p.is_dir()).map(|(p, b)| blocked(t(p, true, Filter::All, 0), b)).collect()
}

/// Cachés que los programadores pueden regenerar en cualquier momento.
fn dev_targets(env: &Env) -> Vec<Target> {
    let h = home();
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut list = vec![h.join(".npm/_cacache"), env.local.join("pip"), env.local.join("yarn"), env.local.join("go-build")];
    #[cfg(target_os = "macos")]
    list.extend([h.join("Library/Developer/Xcode/DerivedData"), h.join("Library/Developer/CoreSimulator/Caches")]);
    dirs(list).into_iter().map(|p| t(p, true, Filter::All, 24)).collect()
}

pub fn categories(env: &Env) -> Vec<CategoryDef> {
    let h = home();
    let mut v = vec![CategoryDef {
        id: "temp_user",
        name: "Temporales de usuario",
        desc: "Archivos que los programas dejan en la carpeta temporal. Solo los de más de 24 h.",
        kind: Kind::Safe,
        special: Special::None,
        default_on: true,
        targets: vec![t(env.temp.clone(), true, Filter::All, 24)],
    }];

    #[cfg(target_os = "macos")]
    {
        v.push(CategoryDef {
            id: "apps",
            name: "Caché de aplicaciones",
            desc: "~/Library/Caches: copias que las apps guardan para abrir más rápido. Solo lo de más de 24 h.",
            kind: Kind::Safe,
            special: Special::None,
            default_on: true,
            targets: mac_app_caches(env),
        });
        v.push(CategoryDef {
            id: "errors",
            name: "Informes de errores",
            desc: "Reportes de apps que se cerraron mal (DiagnosticReports).",
            kind: Kind::Safe,
            special: Special::None,
            default_on: true,
            targets: dirs(vec![h.join("Library/Logs/DiagnosticReports")]).into_iter().map(|p| t(p, true, Filter::All, 0)).collect(),
        });
        v.push(CategoryDef {
            id: "logs",
            name: "Registros antiguos",
            desc: "Registros de apps (~/Library/Logs) de más de 30 días.",
            kind: Kind::Safe,
            special: Special::None,
            default_on: true,
            targets: dirs(vec![h.join("Library/Logs")]).into_iter().map(|p| t(p, true, Filter::All, 24 * 30)).collect(),
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        v.push(CategoryDef {
            id: "apps",
            name: "Caché de aplicaciones",
            desc: "Discord, VS Code, Spotify, Slack, Teams y miniaturas del explorador de archivos (solo caché).",
            kind: Kind::Safe,
            special: Special::None,
            default_on: true,
            targets: app_targets(env),
        });
        v.push(CategoryDef {
            id: "errors",
            name: "Informes de errores",
            desc: "Reportes de programas que se cerraron mal (/var/crash) de tu usuario.",
            kind: Kind::Safe,
            special: Special::None,
            default_on: true,
            targets: dirs(vec![PathBuf::from("/var/crash")]).into_iter().map(|p| t(p, false, Filter::Ext(&["crash"]), 0)).collect(),
        });
        v.push(CategoryDef {
            id: "shaders",
            name: "Caché de gráficos",
            desc: "Sombreadores de Mesa y NVIDIA. Se regeneran solos.",
            kind: Kind::Safe,
            special: Special::None,
            default_on: true,
            targets: dirs(vec![env.local.join("mesa_shader_cache"), env.local.join("nvidia"), h.join(".nv/GLCache")]).into_iter().map(|p| t(p, true, Filter::All, 0)).collect(),
        });
        v.push(CategoryDef {
            id: "update",
            name: "Caché de paquetes del sistema",
            desc: "Paquetes ya instalados que apt, dnf o pacman guardan. Pide tu contraseña.",
            kind: Kind::Safe,
            special: Special::System("paquetes"),
            default_on: false,
            targets: vec![],
        });
        v.push(CategoryDef {
            id: "journal",
            name: "Registros del sistema (journal)",
            desc: "Registros de systemd de más de 30 días. Pide tu contraseña.",
            kind: Kind::Safe,
            special: Special::System("journal"),
            default_on: false,
            targets: vec![],
        });
    }

    v.push(CategoryDef {
        id: "browsers",
        name: "Caché de navegadores",
        desc: "Chrome, Edge, Brave, Chromium y Firefox. NO toca contraseñas, historial ni sesiones.",
        kind: Kind::Safe,
        special: Special::None,
        default_on: true,
        targets: browser_targets(env),
    });
    v.push(CategoryDef {
        id: "dev",
        name: "Caché de herramientas de programación",
        desc: "npm, pip, yarn, Go y Xcode (DerivedData). Se vuelven a descargar o generar solos.",
        kind: Kind::Safe,
        special: Special::None,
        default_on: true,
        targets: dev_targets(env),
    });
    v.push(CategoryDef {
        id: "recycle",
        name: "Papelera",
        desc: "Lo que tú ya enviaste a la Papelera. Al vaciarla se borra para siempre.",
        kind: Kind::Review,
        special: Special::RecycleBin,
        default_on: false,
        targets: vec![],
    });
    if let Some(s) = &env.screenshots {
        #[cfg(target_os = "macos")]
        let targets = ["captura de pantalla", "screen shot", "screenshot"].iter().map(|p| t(s.clone(), false, Filter::Prefix(p), 0)).collect();
        #[cfg(not(target_os = "macos"))]
        let targets = vec![t(s.clone(), true, Filter::Ext(IMAGE_EXT), 0)];
        v.push(CategoryDef {
            id: "screenshots",
            name: "Capturas de pantalla",
            desc: "Imágenes de capturas de pantalla. Van a la Papelera (recuperables).",
            kind: Kind::Review,
            special: Special::None,
            default_on: false,
            targets,
        });
    }
    if let Some(d) = &env.downloads {
        v.push(CategoryDef {
            id: "downloads_old",
            name: "Instaladores y comprimidos viejos",
            desc: "En Descargas: .dmg, .pkg, .deb, .rpm, .AppImage, .zip, .tar.gz… de más de 30 días. Van a la Papelera.",
            kind: Kind::Review,
            special: Special::None,
            default_on: false,
            targets: vec![t(d.clone(), false, Filter::Ext(UNIX_PACKAGE_EXT), 24 * 30)],
        });
    }
    v.push(CategoryDef {
        id: "protected",
        name: "Videos y documentos",
        desc: "Protegidos. Nunca se borran solos: solo los que tú marques, y van a la Papelera.",
        kind: Kind::Protected,
        special: Special::None,
        default_on: false,
        targets: env.downloads.iter().map(|d| t(d.clone(), true, Filter::ProtectedOnly, 24 * 60)).collect(),
    });
    v
}

const UNIX_PACKAGE_EXT: &[&str] = &["dmg", "pkg", "deb", "rpm", "appimage", "run", "zip", "rar", "7z", "iso", "tar", "gz", "tgz", "xz", "bz2", "exe", "msi"];

// ───────────── Categorías que limpia un comando del sistema (solo Linux) ─────────────

fn which(cmd: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {} >/dev/null 2>&1", cmd)]).status().map(|s| s.success()).unwrap_or(false)
}

fn dir_size(p: &Path) -> (u64, u64) {
    let (mut s, mut n) = (0, 0);
    for e in walkdir::WalkDir::new(p).follow_links(false).into_iter().flatten() {
        if let Ok(m) = e.metadata() {
            if m.is_file() && m.len() > 0 {
                s += m.len();
                n += 1;
            }
        }
    }
    (s, n)
}

/// (bytes, elementos) que liberaría la categoría. Solo lee.
pub fn system_size(id: &str) -> (u64, u64) {
    match id {
        "paquetes" => {
            let mut total = (0, 0);
            for d in ["/var/cache/apt/archives", "/var/cache/dnf", "/var/cache/pacman/pkg"] {
                let (s, n) = dir_size(Path::new(d));
                total = (total.0 + s, total.1 + n);
            }
            total
        }
        "journal" => {
            // «Archived and active journals take up 1.2G in the file system.»
            let out = Command::new("journalctl").arg("--disk-usage").output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
            let size = out
                .split_whitespace()
                .find_map(|w| {
                    let w = w.trim_end_matches('.');
                    let (num, unit) = w.split_at(w.find(|c: char| c.is_ascii_alphabetic())?);
                    let n: f64 = num.parse().ok()?;
                    let mul = match unit.chars().next()? {
                        'K' => 1024.0,
                        'M' => 1048576.0,
                        'G' => 1073741824.0,
                        'B' => 1.0,
                        _ => return None,
                    };
                    Some((n * mul) as u64)
                })
                .unwrap_or(0);
            // Se conservan los últimos 30 días: se estima que se libera la mitad.
            (size / 2, u64::from(size > 0))
        }
        _ => (0, 0),
    }
}

/// Limpia la categoría con el comando del sistema (pide la contraseña). Devuelve lo liberado.
pub fn system_clean(id: &str) -> Result<u64, String> {
    let before = system_size(id).0;
    let cmd = match id {
        "paquetes" if which("apt-get") => "apt-get clean",
        "paquetes" if which("dnf") => "dnf clean packages",
        "paquetes" if which("pacman") => "paccache -rk1 2>/dev/null || pacman -Sc --noconfirm",
        "journal" => "journalctl --vacuum-time=30d",
        _ => return Err("No disponible en este sistema.".into()),
    };
    crate::sys::run_as_admin(cmd)?;
    Ok(before.saturating_sub(system_size(id).0))
}
