use std::path::PathBuf;

use windows_sys::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music, FOLDERID_Pictures,
    FOLDERID_Screenshots, FOLDERID_Videos,
};

use super::*;
use crate::sys;

fn jb_env_path(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(fallback))
}

pub fn jb_detect() -> JbEnv {
    let profile = jb_env_path("USERPROFILE", "C:\\Users\\Default");
    let pictures = sys::jb_known_folder(&FOLDERID_Pictures);
    let screenshots = sys::jb_known_folder(&FOLDERID_Screenshots)
        .or_else(|| pictures.as_ref().map(|p| p.join("Screenshots")));
    let downloads = sys::jb_known_folder(&FOLDERID_Downloads);

    let mut forbidden: Vec<PathBuf> = [
        &FOLDERID_Documents,
        &FOLDERID_Desktop,
        &FOLDERID_Videos,
        &FOLDERID_Music,
        &FOLDERID_Pictures,
        &FOLDERID_Downloads,
    ]
    .iter()
    .filter_map(|id| sys::jb_known_folder(id))
    .collect();
    for var in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(p) = std::env::var_os(var) {
            forbidden.push(PathBuf::from(p));
        }
    }
    if let Ok(rd) = std::fs::read_dir(&profile) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().to_lowercase().starts_with("onedrive") {
                forbidden.push(e.path());
            }
        }
    }
    for name in ["Documents", "Desktop", "Videos", "Music", "Pictures", "Downloads"] {
        forbidden.push(profile.join(name));
    }

    JbEnv {
        local: jb_env_path("LOCALAPPDATA", "C:\\Users\\Default\\AppData\\Local"),
        roaming: jb_env_path("APPDATA", "C:\\Users\\Default\\AppData\\Roaming"),
        temp: std::env::temp_dir(),
        windir: jb_env_path("WINDIR", "C:\\Windows"),
        programdata: jb_env_path("ProgramData", "C:\\ProgramData"),
        downloads,
        screenshots,
        forbidden,
    }
}

const CHROMIUM: &[(&str, &[&str])] = &[
    ("Google\\Chrome\\User Data", &["chrome.exe"]),
    ("Microsoft\\Edge\\User Data", &["msedge.exe"]),
    ("BraveSoftware\\Brave-Browser\\User Data", &["brave.exe"]),
    ("Vivaldi\\User Data", &["vivaldi.exe"]),
    ("Opera Software\\Opera Stable", &["opera.exe"]),
    ("Opera Software\\Opera GX Stable", &["opera.exe"]),
];

const CACHE_DIRS: &[&str] = &["Cache", "Code Cache", "GPUCache"];

fn jb_browser_targets(env: &JbEnv) -> Vec<JbTarget> {
    let mut out = Vec::new();
    for (rel, procs) in CHROMIUM {
        let base = env.local.join(rel);
        if !base.is_dir() {
            continue;
        }
        let mut profiles = vec![base.clone()];
        if let Ok(rd) = std::fs::read_dir(&base) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n == "Default" || n.starts_with("Profile ") || n == "Guest Profile" {
                    profiles.push(e.path());
                }
            }
        }
        for p in profiles {
            for c in CACHE_DIRS {
                let d = p.join(c);
                if d.is_dir() {
                    out.push(jb_blocked(jb_t(d, true, JbFilter::All, 0), procs));
                }
            }
        }
        for c in ["GrShaderCache", "ShaderCache"] {
            let d = base.join(c);
            if d.is_dir() {
                out.push(jb_blocked(jb_t(d, true, JbFilter::All, 0), procs));
            }
        }
    }
    let ff = env.local.join("Mozilla\\Firefox\\Profiles");
    if let Ok(rd) = std::fs::read_dir(&ff) {
        for e in rd.flatten() {
            for c in ["cache2", "startupCache", "thumbnails"] {
                let d = e.path().join(c);
                if d.is_dir() {
                    out.push(jb_blocked(jb_t(d, true, JbFilter::All, 0), &["firefox.exe"]));
                }
            }
        }
    }
    out
}

fn jb_app_cache_targets(env: &JbEnv) -> Vec<JbTarget> {
    let r = &env.roaming;
    let l = &env.local;
    let list: Vec<(PathBuf, &'static [&'static str])> = vec![
        (r.join("discord\\Cache"), &["discord.exe"]),
        (r.join("discord\\Code Cache"), &["discord.exe"]),
        (r.join("discord\\GPUCache"), &["discord.exe"]),
        (r.join("Code\\Cache"), &["code.exe"]),
        (r.join("Code\\CachedData"), &["code.exe"]),
        (r.join("Code\\Code Cache"), &["code.exe"]),
        (r.join("Code\\GPUCache"), &["code.exe"]),
        (l.join("Spotify\\Data"), &["spotify.exe"]),
        (r.join("Microsoft\\Teams\\Cache"), &["teams.exe", "ms-teams.exe"]),
        (r.join("Slack\\Cache"), &["slack.exe"]),
        (r.join("Slack\\Code Cache"), &["slack.exe"]),
        (r.join("Zoom\\data\\Cache"), &["zoom.exe"]),
        (r.join("WhatsApp\\Cache"), &["whatsapp.exe"]),
    ];
    list.into_iter()
        .filter(|(p, _)| p.is_dir())
        .map(|(p, b)| jb_blocked(jb_t(p, true, JbFilter::All, 0), b))
        .collect()
}

const UPDATE_PROCS: &[&str] = &["tiworker.exe", "trustedinstaller.exe", "wuauclt.exe"];

pub fn jb_categories(env: &JbEnv) -> Vec<JbCategoryDef> {
    let w = &env.windir;
    let l = &env.local;
    let mut v = vec![
        JbCategoryDef {
            id: "temp_user",
            name: "Temporales de usuario",
            desc: "Archivos que los programas dejan en %TEMP%. Solo los de más de 24 h.",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: vec![jb_t(env.temp.clone(), true, JbFilter::All, 24)],
        },
        JbCategoryDef {
            id: "temp_win",
            name: "Temporales de Windows",
            desc: "Carpeta Windows\\Temp. Solo archivos de más de 24 h.",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: vec![jb_admin(jb_t(w.join("Temp"), true, JbFilter::All, 24))],
        },
        JbCategoryDef {
            id: "thumbs",
            name: "Caché de miniaturas",
            desc: "Vistas previas de imágenes del Explorador. Windows las vuelve a crear.",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: vec![jb_t(l.join("Microsoft\\Windows\\Explorer"), false, JbFilter::Prefix("thumbcache_"), 0)],
        },
        JbCategoryDef {
            id: "errors",
            name: "Informes de errores y volcados",
            desc: "Reportes de fallos y volcados de memoria de programas que se cerraron mal.",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: vec![
                jb_t(l.join("Microsoft\\Windows\\WER"), true, JbFilter::All, 0),
                jb_t(l.join("CrashDumps"), true, JbFilter::All, 0),
                jb_admin(jb_t(env.programdata.join("Microsoft\\Windows\\WER"), true, JbFilter::All, 0)),
                jb_admin(jb_t(w.join("Minidump"), true, JbFilter::All, 0)),
                jb_admin(jb_t(w.join("LiveKernelReports"), true, JbFilter::Ext(&["dmp"]), 0)),
            ],
        },
        JbCategoryDef {
            id: "browsers",
            name: "Caché de navegadores",
            desc: "Chrome, Edge, Brave, Opera, Firefox. NO toca contraseñas, historial ni sesiones.",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: jb_browser_targets(env),
        },
        JbCategoryDef {
            id: "apps",
            name: "Caché de aplicaciones",
            desc: "Discord, VS Code, Spotify, Teams, Slack, Zoom, WhatsApp (solo caché).",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: jb_app_cache_targets(env),
        },
        JbCategoryDef {
            id: "shaders",
            name: "Caché de gráficos",
            desc: "Sombreadores de DirectX, NVIDIA, AMD e Intel. Se regeneran solos.",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: [
                l.join("D3DSCache"),
                l.join("NVIDIA\\DXCache"),
                l.join("NVIDIA\\GLCache"),
                l.join("AMD\\DxCache"),
                l.join("AMD\\GLCache"),
                l.join("Intel\\ShaderCache"),
            ]
            .into_iter()
            .filter(|p| p.is_dir())
            .map(|p| jb_t(p, true, JbFilter::All, 0))
            .collect(),
        },
        JbCategoryDef {
            id: "update",
            name: "Caché de Windows Update",
            desc: "Actualizaciones ya instaladas y caché de Optimización de distribución.",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: vec![
                jb_blocked(jb_admin(jb_t(w.join("SoftwareDistribution\\Download"), true, JbFilter::All, 72)), UPDATE_PROCS),
                jb_admin(jb_t(
                    w.join("ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache"),
                    true,
                    JbFilter::All,
                    0,
                )),
            ],
        },
        JbCategoryDef {
            id: "logs",
            name: "Registros antiguos del sistema",
            desc: "Logs de Windows de más de 30 días.",
            kind: JbKind::Safe,
            special: JbSpecial::None,
            default_on: true,
            targets: vec![jb_admin(jb_t(w.join("Logs"), true, JbFilter::Ext(&["log", "etl", "old", "cab"]), 24 * 30))],
        },
        JbCategoryDef {
            id: "recycle",
            name: "Papelera de reciclaje",
            desc: "Lo que tú ya enviaste a la Papelera. Al vaciarla se borra para siempre.",
            kind: JbKind::Review,
            special: JbSpecial::RecycleBin,
            default_on: false,
            targets: vec![],
        },
    ];

    if let Some(s) = &env.screenshots {
        v.push(JbCategoryDef {
            id: "screenshots",
            name: "Capturas de pantalla",
            desc: "Imágenes de la carpeta Capturas. Van a la Papelera (recuperables).",
            kind: JbKind::Review,
            special: JbSpecial::None,
            default_on: false,
            targets: vec![jb_t(s.clone(), true, JbFilter::Ext(IMAGE_EXT), 0)],
        });
    }
    if let Some(d) = &env.downloads {
        v.push(JbCategoryDef {
            id: "downloads_old",
            name: "Instaladores y comprimidos viejos",
            desc: "En Descargas: .exe, .msi, .zip, .rar, .iso... de más de 30 días. Van a la Papelera.",
            kind: JbKind::Review,
            special: JbSpecial::None,
            default_on: false,
            targets: vec![jb_t(d.clone(), false, JbFilter::Ext(PACKAGE_EXT), 24 * 30)],
        });
    }
    v.push(JbCategoryDef {
        id: "protected",
        name: "Videos y documentos",
        desc: "Protegidos. Nunca se borran solos: solo los que tú marques, y van a la Papelera.",
        kind: JbKind::Protected,
        special: JbSpecial::None,
        default_on: false,
        targets: env
            .downloads
            .iter()
            .map(|d| jb_t(d.clone(), true, JbFilter::ProtectedOnly, 24 * 60))
            .collect(),
    });
    v
}

pub fn jb_system_size(_id: &str) -> (u64, u64) {
    (0, 0)
}

pub fn jb_system_clean(_id: &str) -> Result<u64, String> {
    Err("No disponible en Windows.".into())
}
