// ════════════════════════════════════════════════════════════════════════
//  NEXO · nexo/src/unix/rules.rs
//  Autor: Chatta Betanzos, Jimy Alexander  ·  jimy-betanzos
// ════════════════════════════════════════════════════════════════════════

//! Reglas de Linux y macOS: carpetas de basura conocidas y carpetas personales prohibidas.
//!
//! Las mismas clases y garantías que en Windows: la basura segura se borra, lo del usuario
//! va a la Papelera y los videos y documentos nunca se borran solos. Los identificadores
//! (temp_user, browsers, apps…) coinciden con los de Windows para que la configuración del
//! modo automático sirva en cualquier sistema.

// jimy-betanzos · dependencia: biblioteca estándar de Rust (path)
use std::path::{Path, PathBuf};
// jimy-betanzos · dependencia: biblioteca estándar de Rust (process)
use std::process::Command;

// jimy-betanzos · dependencia: módulo propio de la interfaz
use super::*;

/// jimy-betanzos · Carpeta personal del usuario.
fn jb_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// jimy-betanzos · Carpeta del usuario según ~/.config/user-dirs.dirs (Linux; en español suele ser
/// «Documentos», «Descargas»…). Si no está definida, se usa el nombre en inglés.
#[cfg(not(target_os = "macos"))]
fn jb_xdg_dir(key: &str, default: &str) -> PathBuf {
    let h = jb_home();
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

/// jimy-betanzos · Detecta las carpetas del usuario y del sistema.
pub fn jb_detect() -> JbEnv {
    let h = jb_home();
    #[cfg(target_os = "macos")]
    let (docs, desk, vids, music, pics, down) = (h.join("Documents"), h.join("Desktop"), h.join("Movies"), h.join("Music"), h.join("Pictures"), h.join("Downloads"));
    #[cfg(not(target_os = "macos"))]
    let (docs, desk, vids, music, pics, down) = (
        jb_xdg_dir("DOCUMENTS", "Documents"),
        jb_xdg_dir("DESKTOP", "Desktop"),
        jb_xdg_dir("VIDEOS", "Videos"),
        jb_xdg_dir("MUSIC", "Music"),
        jb_xdg_dir("PICTURES", "Pictures"),
        jb_xdg_dir("DOWNLOAD", "Downloads"),
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

    JbEnv {
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

/// jimy-betanzos · Deja solo las carpetas que existen.
fn jb_dirs(list: Vec<PathBuf>) -> Vec<PathBuf> {
    list.into_iter().filter(|p| p.is_dir()).collect()
}

/// jimy-betanzos · Subcarpetas `Cache`, `Code Cache` y `GPUCache` de los perfiles de un navegador Chromium.
fn jb_chromium_caches(base: &Path, procs: &'static [&'static str]) -> Vec<JbTarget> {
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
                out.push(jb_blocked(jb_t(d, true, JbFilter::All, 0), procs));
            }
        }
    }
    out
}

/// jimy-betanzos · Carpetas de caché de los perfiles de Firefox.
fn jb_firefox_caches(profiles_dir: &Path) -> Vec<JbTarget> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(profiles_dir) {
        for e in rd.flatten() {
            for c in ["cache2", "startupCache", "thumbnails"] {
                let d = e.path().join(c);
                if d.is_dir() {
                    out.push(jb_blocked(jb_t(d, true, JbFilter::All, 0), &["firefox", "firefox-bin"]));
                }
            }
        }
    }
    out
}

/// jimy-betanzos · Carpetas de caché de los navegadores.
fn jb_browser_targets(env: &JbEnv) -> Vec<JbTarget> {
    let c = &env.local;
    let mut out = Vec::new();
    #[cfg(target_os = "macos")]
    {
        out.extend(jb_chromium_caches(&c.join("Google/Chrome"), &["google chrome", "google chrome helper"]));
        out.extend(jb_chromium_caches(&c.join("Microsoft Edge"), &["microsoft edge"]));
        out.extend(jb_chromium_caches(&c.join("BraveSoftware/Brave-Browser"), &["brave browser"]));
        out.extend(jb_firefox_caches(&c.join("Firefox/Profiles")));
    }
    #[cfg(not(target_os = "macos"))]
    {
        out.extend(jb_chromium_caches(&c.join("google-chrome"), &["chrome", "google-chrome"]));
        out.extend(jb_chromium_caches(&c.join("chromium"), &["chromium", "chromium-browser"]));
        out.extend(jb_chromium_caches(&c.join("microsoft-edge"), &["msedge", "microsoft-edge"]));
        out.extend(jb_chromium_caches(&c.join("BraveSoftware/Brave-Browser"), &["brave", "brave-browser"]));
        out.extend(jb_firefox_caches(&c.join("mozilla/firefox")));
        out.extend(jb_firefox_caches(&jb_home().join(".mozilla/firefox")));
    }
    out
}

/// jimy-betanzos · Cada carpeta de ~/Library/Caches por separado, sin las de los navegadores (tienen su
/// propia categoría) ni las de Apple (las gestiona macOS).
#[cfg(target_os = "macos")]
fn jb_mac_app_caches(env: &JbEnv) -> Vec<JbTarget> {
    const SKIP: &[&str] = &["Google", "Firefox", "Microsoft Edge", "BraveSoftware", "com.google.Chrome", "org.mozilla.firefox"];
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&env.local) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if e.path().is_dir() && !n.starts_with("com.apple.") && !SKIP.contains(&n.as_str()) {
                out.push(jb_t(e.path(), true, JbFilter::All, 24));
            }
        }
    }
    out
}

/// jimy-betanzos · Carpetas de caché de aplicaciones conocidas.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn jb_app_targets(env: &JbEnv) -> Vec<JbTarget> {
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
    list.into_iter().filter(|(p, _)| p.is_dir()).map(|(p, b)| jb_blocked(jb_t(p, true, JbFilter::All, 0), b)).collect()
}

/// jimy-betanzos · Cachés que los programadores pueden regenerar en cualquier momento.
fn jb_dev_targets(env: &JbEnv) -> Vec<JbTarget> {
    let h = jb_home();
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut list = vec![h.join(".npm/_cacache"), env.local.join("pip"), env.local.join("yarn"), env.local.join("go-build")];
    #[cfg(target_os = "macos")]
    list.extend([h.join("Library/Developer/Xcode/DerivedData"), h.join("Library/Developer/CoreSimulator/Caches")]);
    jb_dirs(list).into_iter().map(|p| jb_t(p, true, JbFilter::All, 24)).collect()
}

/// jimy-betanzos · Lista completa de categorías de limpieza.
pub fn jb_categories(env: &JbEnv) -> Vec<JbCategoryDef> {
    let h = jb_home();
    let mut v = vec![JbCategoryDef {
        id: "temp_user",
        name: "Temporales de usuario",
        desc: "Archivos que los programas dejan en la carpeta temporal. Solo los de más de 24 h.",
        kind: JbKind::Safe,
        special: JbSpecial::None,
        default_on: true,
        targets: vec![jb_t(env.temp.clone(), true, JbFilter::All, 24)],
    }];

    #[cfg(target_os = "macos")]
    {
        v.push(JbCategoryDef {
            id: "apps",
            name: "Caché de aplicaciones",
            desc: "~/Library/Caches: copias que las apps guardan para abrir más rápido. Solo lo de más de 24 h.",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: jb_mac_app_caches(env),
        });
        v.push(JbCategoryDef {
            id: "errors",
            name: "Informes de errores",
            desc: "Reportes de apps que se cerraron mal (DiagnosticReports).",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: jb_dirs(vec![h.join("Library/Logs/DiagnosticReports")]).into_iter().map(|p| jb_t(p, true, JbFilter::All, 0)).collect(),
        });
        v.push(JbCategoryDef {
            id: "logs",
            name: "Registros antiguos",
            desc: "Registros de apps (~/Library/Logs) de más de 30 días.",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: jb_dirs(vec![h.join("Library/Logs")]).into_iter().map(|p| jb_t(p, true, JbFilter::All, 24 * 30)).collect(),
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        v.push(JbCategoryDef {
            id: "apps",
            name: "Caché de aplicaciones",
            desc: "Discord, VS Code, Spotify, Slack, Teams y miniaturas del explorador de archivos (solo caché).",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: jb_app_targets(env),
        });
        v.push(JbCategoryDef {
            id: "errors",
            name: "Informes de errores",
            desc: "Reportes de programas que se cerraron mal (/var/crash) de tu usuario.",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: jb_dirs(vec![PathBuf::from("/var/crash")]).into_iter().map(|p| jb_t(p, false, JbFilter::Ext(&["crash"]), 0)).collect(),
        });
        v.push(JbCategoryDef {
            id: "shaders",
            name: "Caché de gráficos",
            desc: "Sombreadores de Mesa y NVIDIA. Se regeneran solos.",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: jb_dirs(vec![env.local.join("mesa_shader_cache"), env.local.join("nvidia"), h.join(".nv/GLCache")]).into_iter().map(|p| jb_t(p, true, JbFilter::All, 0)).collect(),
        });
        v.push(JbCategoryDef {
            id: "update",
            name: "Caché de paquetes del sistema",
            desc: "Paquetes ya instalados que apt, dnf o pacman guardan. Pide tu contraseña.",
            kind: JbKind::Safe,
            special: JbSpecial::System("paquetes"),
            default_on: false,
            targets: vec![],
        });
        v.push(JbCategoryDef {
            id: "journal",
            name: "Registros del sistema (journal)",
            desc: "Registros de systemd de más de 30 días. Pide tu contraseña.",
            kind: JbKind::Safe,
            special: JbSpecial::System("journal"),
            default_on: false,
            targets: vec![],
        });
    }

    v.push(JbCategoryDef {
        id: "browsers",
        name: "Caché de navegadores",
        desc: "Chrome, Edge, Brave, Chromium y Firefox. NO toca contraseñas, historial ni sesiones.",
        kind: JbKind::Safe,
        special: JbSpecial::None,
        default_on: true,
        targets: jb_browser_targets(env),
    });
    v.push(JbCategoryDef {
        id: "dev",
        name: "Caché de herramientas de programación",
        desc: "npm, pip, yarn, Go y Xcode (DerivedData). Se vuelven a descargar o generar solos.",
        kind: JbKind::Safe,
        special: JbSpecial::None,
        default_on: true,
        targets: jb_dev_targets(env),
    });
    v.push(JbCategoryDef {
        id: "recycle",
        name: "Papelera",
        desc: "Lo que tú ya enviaste a la Papelera. Al vaciarla se borra para siempre.",
        kind: JbKind::Review,
        special: JbSpecial::RecycleBin,
        default_on: false,
        targets: vec![],
    });
    if let Some(s) = &env.screenshots {
        #[cfg(target_os = "macos")]
        let targets = ["captura de pantalla", "screen shot", "screenshot"].iter().map(|p| jb_t(s.clone(), false, JbFilter::Prefix(p), 0)).collect();
        #[cfg(not(target_os = "macos"))]
        let targets = vec![jb_t(s.clone(), true, JbFilter::Ext(IMAGE_EXT), 0)];
        v.push(JbCategoryDef {
            id: "screenshots",
            name: "Capturas de pantalla",
            desc: "Imágenes de capturas de pantalla. Van a la Papelera (recuperables).",
            kind: JbKind::Review,
            special: JbSpecial::None,
            default_on: false,
            targets,
        });
    }
    if let Some(d) = &env.downloads {
        v.push(JbCategoryDef {
            id: "downloads_old",
            name: "Instaladores y comprimidos viejos",
            desc: "En Descargas: .dmg, .pkg, .deb, .rpm, .AppImage, .zip, .tar.gz… de más de 30 días. Van a la Papelera.",
            kind: JbKind::Review,
            special: JbSpecial::None,
            default_on: false,
            targets: vec![jb_t(d.clone(), false, JbFilter::Ext(UNIX_PACKAGE_EXT), 24 * 30)],
        });
    }
    v.push(JbCategoryDef {
        id: "protected",
        name: "Videos y documentos",
        desc: "Protegidos. Nunca se borran solos: solo los que tú marques, y van a la Papelera.",
        kind: JbKind::Protected,
        special: JbSpecial::None,
        default_on: false,
        targets: env.downloads.iter().map(|d| jb_t(d.clone(), true, JbFilter::ProtectedOnly, 24 * 60)).collect(),
    });
    v
}

const UNIX_PACKAGE_EXT: &[&str] = &["dmg", "pkg", "deb", "rpm", "appimage", "run", "zip", "rar", "7z", "iso", "tar", "gz", "tgz", "xz", "bz2", "exe", "msi"];

// ───────────── Categorías que limpia un comando del sistema (solo Linux) ─────────────

/// jimy-betanzos · Indica si un comando está instalado.
fn jb_which(cmd: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {} >/dev/null 2>&1", cmd)]).status().map(|s| s.success()).unwrap_or(false)
}

/// jimy-betanzos · Tamaño y cantidad de archivos de una carpeta.
fn jb_dir_size(p: &Path) -> (u64, u64) {
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

/// jimy-betanzos · (bytes, elementos) que liberaría la categoría. Solo lee.
pub fn jb_system_size(id: &str) -> (u64, u64) {
    match id {
        "paquetes" => {
            let mut total = (0, 0);
            for d in ["/var/cache/apt/archives", "/var/cache/dnf", "/var/cache/pacman/pkg"] {
                let (s, n) = jb_dir_size(Path::new(d));
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

/// jimy-betanzos · Limpia la categoría con el comando del sistema (pide la contraseña). Devuelve lo liberado.
pub fn jb_system_clean(id: &str) -> Result<u64, String> {
    let before = jb_system_size(id).0;
    let cmd = match id {
        "paquetes" if jb_which("apt-get") => "apt-get clean",
        "paquetes" if jb_which("dnf") => "dnf clean packages",
        "paquetes" if jb_which("pacman") => "paccache -rk1 2>/dev/null || pacman -Sc --noconfirm",
        "journal" => "journalctl --vacuum-time=30d",
        _ => return Err("No disponible en este sistema.".into()),
    };
    crate::sys::jb_run_as_admin(cmd)?;
    Ok(before.saturating_sub(jb_system_size(id).0))
}
