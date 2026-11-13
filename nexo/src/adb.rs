//! Modo potenciado (fase 4): NEXO controla los celulares desde la laptop con ADB,
//! por USB o por WiFi local (depuración inalámbrica de Android 11). Sin internet.
//!
//! Hace lo que una app sola no puede en Android 11: borrar la caché de todas las
//! apps, desactivar apps preinstaladas (reversible) y medir la batería por app.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::{install, sys};

pub fn adb_path() -> Option<PathBuf> {
    let bin = if cfg!(windows) { "adb.exe" } else { "adb" };
    let mut c: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            c.push(d.join("platform-tools").join(bin));
        }
    }
    c.push(install::install_dir().join("platform-tools").join(bin));
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(v) = std::env::var_os(var) {
            c.push(PathBuf::from(v).join("platform-tools").join(bin));
        }
    }
    // Ubicación por defecto del SDK de Android Studio en cada sistema.
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        c.push(PathBuf::from(l).join("Android").join("Sdk").join("platform-tools").join(bin));
    }
    if let Some(h) = std::env::var_os("HOME") {
        let h = PathBuf::from(h);
        c.push(h.join("Android/Sdk/platform-tools").join(bin));
        c.push(h.join("Library/Android/sdk/platform-tools").join(bin));
    }
    // Instalado con el gestor de paquetes (apt install adb, brew install android-platform-tools).
    for d in ["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin"] {
        c.push(PathBuf::from(d).join(bin));
    }
    c.into_iter().find(|p| p.exists())
}

fn run(args: &[&str]) -> Result<String, String> {
    let adb = adb_path().ok_or("No se encontró ADB")?;
    let out = sys::hidden_cmd(&adb.to_string_lossy()).args(args).output().map_err(|e| e.to_string())?;
    let so = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        Ok(so)
    } else {
        let se = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if se.is_empty() { so.trim().to_string() } else { se })
    }
}

fn shell(serial: &str, cmd: &str) -> Result<String, String> {
    run(&["-s", serial, "shell", cmd])
}

#[derive(Clone, Default)]
pub struct Dev {
    pub serial: String,
    pub state: String,
    pub model: String,
}

impl Dev {
    pub fn ready(&self) -> bool {
        self.state == "device"
    }
    pub fn wifi(&self) -> bool {
        self.serial.contains(':') || self.serial.contains("._adb-tls-connect")
    }
}

pub fn devices() -> Result<Vec<Dev>, String> {
    let out = run(&["devices", "-l"])?;
    Ok(out
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty() && !l.starts_with('*'))
        .map(|l| {
            let mut it = l.split_whitespace();
            let serial = it.next().unwrap_or("").to_string();
            let state = it.next().unwrap_or("").to_string();
            let model = l
                .split_whitespace()
                .find_map(|t| t.strip_prefix("model:"))
                .unwrap_or("")
                .replace('_', " ");
            Dev { serial, state, model }
        })
        .collect())
}

pub fn pair(addr: &str, code: &str) -> Result<String, String> {
    let out = run(&["pair", addr.trim(), code.trim()])?;
    if out.to_lowercase().contains("success") { Ok("Vinculado correctamente.".into()) } else { Err(out.trim().to_string()) }
}

pub fn connect(addr: &str) -> Result<String, String> {
    let out = run(&["connect", addr.trim()])?;
    if out.contains("connected") { Ok("Conectado.".into()) } else { Err(out.trim().to_string()) }
}

#[derive(Clone, Default)]
pub struct Info {
    pub brand: String,
    pub model: String,
    pub android: String,
    pub battery: u32,
    pub temp_c: f32,
    pub health: String,
    pub data_free: u64,
    pub data_total: u64,
    pub ram_total: u64,
    pub ram_avail: u64,
}

fn kv(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_string()))
}

fn data_space(serial: &str) -> (u64, u64) {
    let out = shell(serial, "df -k /data").unwrap_or_default();
    let line = out.lines().nth(1).unwrap_or("");
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() >= 4 {
        let total = cols[1].parse::<u64>().unwrap_or(0) * 1024;
        let avail = cols[3].parse::<u64>().unwrap_or(0) * 1024;
        (avail, total)
    } else {
        (0, 0)
    }
}

pub fn info(serial: &str) -> Result<Info, String> {
    let props = shell(serial, "getprop ro.product.brand; getprop ro.product.model; getprop ro.build.version.release")?;
    let mut p = props.lines().map(|s| s.trim().to_string());
    let bat = shell(serial, "dumpsys battery").unwrap_or_default();
    let mem = shell(serial, "cat /proc/meminfo").unwrap_or_default();
    let kb = |k: &str| kv(&mem, k).and_then(|v| v.split_whitespace().next().and_then(|n| n.parse::<u64>().ok())).unwrap_or(0) * 1024;
    let (data_free, data_total) = data_space(serial);
    Ok(Info {
        brand: p.next().unwrap_or_default(),
        model: p.next().unwrap_or_default(),
        android: p.next().unwrap_or_default(),
        battery: kv(&bat, "level:").and_then(|v| v.parse().ok()).unwrap_or(0),
        temp_c: kv(&bat, "temperature:").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 10.0,
        health: match kv(&bat, "health:").as_deref() {
            Some("2") => "buena",
            Some("3") => "sobrecalentada",
            Some("4") => "agotada",
            Some("7") => "fría",
            _ => "sin datos",
        }
        .into(),
        data_free,
        data_total,
        ram_total: kb("MemTotal:"),
        ram_avail: kb("MemAvailable:"),
    })
}

/// Borra la caché de TODAS las apps (no toca datos, cuentas ni archivos). Devuelve lo liberado.
pub fn trim_caches(serial: &str) -> Result<u64, String> {
    let (before, _) = data_space(serial);
    shell(serial, "pm trim-caches 999G")?;
    let (after, _) = data_space(serial);
    Ok(after.saturating_sub(before))
}

/// Apps preinstaladas que se pueden desactivar sin riesgo (y volver a activar cuando quieras).
const BLOAT: &[(&str, &str, &str)] = &[
    ("com.facebook.appmanager", "Facebook App Manager", "Instalador de Facebook preinstalado: se actualiza solo y gasta datos."),
    ("com.facebook.services", "Facebook Services", "Servicio de Facebook en segundo plano."),
    ("com.facebook.system", "Facebook App Installer", "Instalador preinstalado de Facebook."),
    ("com.netflix.partner.activation", "Netflix (activación)", "Servicio de socio preinstalado."),
    ("com.opos.ads", "Anuncios del sistema (realme)", "Muestra publicidad en apps del sistema."),
    ("com.heytap.pictorial", "Revista de bloqueo (realme)", "Fondos con anuncios en la pantalla de bloqueo; descarga imágenes."),
    ("com.nearme.statistics.rom", "Estadísticas de uso (realme)", "Envía estadísticas de uso al fabricante."),
    ("com.heytap.browser", "Navegador de realme", "Navegador con anuncios; Chrome hace lo mismo."),
    ("com.heytap.music", "Música de realme", "Reproductor preinstalado."),
    ("com.coloros.video", "Videos de realme", "Reproductor preinstalado."),
    ("com.heytap.cloud", "Nube de realme (HeyTap)", "Copia en la nube del fabricante."),
    ("com.coloros.compass2", "Brújula de realme", "Si no la usas."),
    ("com.hihonor.search", "Búsqueda de Honor", "Buscador del sistema con sugerencias."),
    ("com.hihonor.mediacenter", "Música de Honor", "Reproductor preinstalado."),
    ("com.hihonor.himovie", "Videos de Honor", "Reproductor preinstalado."),
    ("com.hihonor.tips", "Consejos de Honor", "Notificaciones de consejos."),
    ("com.hihonor.health", "Salud de Honor", "Solo útil con reloj o pulsera Honor."),
    ("com.huawei.search", "Búsqueda de Huawei", "Buscador del sistema con sugerencias."),
    ("com.google.android.apps.tachyon", "Google Meet (Duo)", "Si no haces videollamadas con Meet."),
    ("com.google.android.videos", "Google TV", "Tienda de películas."),
    ("com.google.android.apps.youtube.music", "YouTube Music", "Si no lo usas."),
    ("com.google.android.apps.subscriptions.red", "Google One", "Gestor de suscripción."),
    ("com.google.android.apps.magazines", "Google Noticias", "Si no lo usas."),
];

#[derive(Clone)]
pub struct Bloat {
    pub pkg: String,
    pub name: String,
    pub why: String,
    pub enabled: bool,
}

pub fn bloat(serial: &str) -> Result<Vec<Bloat>, String> {
    let all = shell(serial, "pm list packages")?;
    let disabled = shell(serial, "pm list packages -d").unwrap_or_default();
    let has = |list: &str, pkg: &str| list.lines().any(|l| l.trim() == format!("package:{}", pkg));
    Ok(BLOAT
        .iter()
        .filter(|(pkg, _, _)| has(&all, pkg))
        .map(|(pkg, name, why)| Bloat { pkg: pkg.to_string(), name: name.to_string(), why: why.to_string(), enabled: !has(&disabled, pkg) })
        .collect())
}

pub fn set_enabled(serial: &str, pkg: &str, on: bool) -> Result<(), String> {
    let cmd = if on { format!("pm enable {}", pkg) } else { format!("pm disable-user --user 0 {}", pkg) };
    let out = shell(serial, &cmd)?;
    if out.to_lowercase().contains("new state") { Ok(()) } else { Err(out.trim().to_string()) }
}

/// Consumo de batería por app (mAh desde la última carga completa), con datos del sistema.
pub fn battery_usage(serial: &str) -> Result<Vec<(String, f64)>, String> {
    let stats = shell(serial, "dumpsys batterystats --charged")?;
    let pkgs = shell(serial, "cmd package list packages -U").unwrap_or_default();
    let mut by_uid: HashMap<u32, String> = HashMap::new();
    for l in pkgs.lines() {
        let (Some(p), Some(u)) = (l.split_whitespace().find_map(|t| t.strip_prefix("package:")), l.split_whitespace().find_map(|t| t.strip_prefix("uid:"))) else { continue };
        if let Ok(uid) = u.parse::<u32>() {
            by_uid.entry(uid).or_insert_with(|| p.to_string());
        }
    }
    let mut out = Vec::new();
    let mut inside = false;
    for l in stats.lines() {
        let t = l.trim();
        if t.starts_with("Estimated power use") {
            inside = true;
            continue;
        }
        if inside {
            if t.is_empty() {
                break;
            }
            let Some(rest) = t.strip_prefix("Uid ") else { continue };
            let Some((uid_s, val)) = rest.split_once(':') else { continue };
            let mah: f64 = val.split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
            let uid = if let Some(n) = uid_s.strip_prefix("u0a") { n.parse::<u32>().map(|n| 10000 + n).ok() } else { uid_s.parse::<u32>().ok() };
            let name = uid
                .and_then(|u| by_uid.get(&u).cloned())
                .unwrap_or_else(|| if uid_s == "1000" { "Sistema Android".into() } else { format!("uid {}", uid_s) });
            if mah > 0.5 {
                out.push((name, mah));
            }
        }
    }
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out.truncate(10);
    Ok(out)
}

/// APK de NEXO móvil empaquetado junto a NEXO para Windows (si existe).
pub fn mobile_apk() -> Option<PathBuf> {
    let mut c = vec![install::install_dir().join("nexo-movil.apk")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            c.insert(0, d.join("nexo-movil.apk"));
        }
    }
    c.into_iter().find(|p| p.exists())
}

pub fn install_apk(serial: &str, apk: &std::path::Path) -> Result<String, String> {
    let out = run(&["-s", serial, "install", "-r", &apk.to_string_lossy()])?;
    if out.contains("Success") { Ok("NEXO móvil instalado.".into()) } else { Err(out.trim().to_string()) }
}
