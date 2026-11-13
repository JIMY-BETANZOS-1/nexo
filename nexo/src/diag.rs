//! Diagnóstico general (fase 3): nota de 0 a 100 por áreas y recomendaciones.
//!
//! La recogida de datos es de solo lectura (script de PowerShell incluido en el
//! programa). Las únicas acciones que cambian algo son reversibles y las decide el
//! usuario: activar/desactivar programas de inicio, cambiar el plan de energía,
//! lanzar un análisis de Defender.

use serde::{Deserialize, Serialize};

use crate::{config, fmt, sys};

#[cfg(windows)]
#[path = "diag_win.rs"]
mod plat;
#[cfg(unix)]
#[path = "unix/diag.rs"]
mod plat;

pub use plat::{active_power_plan, battery_report, open, quick_scan, quick_scan_available, set_power_plan, set_startup_enabled, setting, txt, BALANCED};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct StartupItem {
    pub name: String,
    pub command: String,
    pub exe: String,
    pub hive: String,
    pub sub: String,
    pub scope: String,
    pub enabled: bool,
    pub signature: String,
    pub publisher: String,
    pub suspicious: bool,
}

impl StartupItem {
    pub fn machine_wide(&self) -> bool {
        self.hive.starts_with("HKLM")
    }
    pub fn missing(&self) -> bool {
        self.signature == "missing"
    }
    pub fn signed(&self) -> bool {
        // "n/a": Linux y macOS no usan firmas Authenticode en los programas de inicio.
        self.signature == "Valid" || self.signature == "n/a"
    }
    /// El propio vigía de NEXO: es de confianza aunque no tenga firma comercial.
    pub fn is_nexo(&self) -> bool {
        let ours = crate::install::installed_exe().to_string_lossy().to_lowercase();
        self.name == "NEXO Vigia" && self.exe.to_lowercase() == ours
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Disk {
    pub name: String,
    pub media: String,
    pub health: String,
    pub size: i64,
    pub wear: Option<i64>,
    pub temp: Option<i64>,
    pub hours: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Battery {
    pub design: i64,
    pub full: i64,
    pub cycles: i64,
}

impl Battery {
    pub fn health(&self) -> Option<f32> {
        (self.design > 0 && self.full > 0).then(|| (self.full as f32 / self.design as f32 * 100.0).min(100.0))
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Defender {
    pub available: bool,
    pub enabled: bool,
    pub realtime: bool,
    pub sig_age: i64,
    pub quick_age: i64,
    pub tamper: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Firewall {
    pub name: String,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Task {
    pub name: String,
    pub path: String,
    pub exe: String,
    pub why: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Raw {
    pub startup: Vec<StartupItem>,
    pub disks: Vec<Disk>,
    pub battery: Battery,
    pub defender: Defender,
    pub firewall: Vec<Firewall>,
    pub uac: Option<bool>,
    pub rdp: bool,
    pub proxy: String,
    pub hosts: Vec<String>,
    pub encryption: i64,
    pub last_update_days: i64,
    pub uptime_days: f64,
    pub os: String,
    pub tasks: Vec<Task>,
    pub tasks_total: i64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub enum Action {
    GoClean,
    GoRam,
    QuickScan,
    SetBalanced,
    BatteryReport,
    Open(String),
}

impl Action {
    pub fn label(&self) -> &'static str {
        match self {
            Action::GoClean => "Ir a Limpieza",
            Action::GoRam => "Ir a Memoria",
            Action::QuickScan => "Analizar ahora",
            Action::SetBalanced => "Usar Equilibrado",
            Action::BatteryReport => "Ver informe",
            Action::Open(t) if t.contains("LenovoCompanion") => "Abrir Vantage",
            Action::Open(t) if t.contains("taskschd") => "Ver tareas",
            Action::Open(_) => "Abrir ajuste",
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Rec {
    /// 0 consejo · 1 conviene revisar · 2 importante
    pub severity: u8,
    pub area: String,
    pub title: String,
    pub detail: String,
    pub action: Option<Action>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Report {
    pub ts: u64,
    pub raw: Raw,
    pub power_guid: String,
    pub power_name: String,
    pub ram_load: u32,
    pub commit: f32,
    pub disk_free: u64,
    pub disk_total: u64,
    pub lenovo: bool,
    pub areas: Vec<(String, Option<u8>)>,
    pub overall: u8,
    pub recs: Vec<Rec>,
}

pub const AREAS: [&str; 5] = ["Seguridad", "Almacenamiento", "Memoria", "Arranque", "Batería"];
const WEIGHTS: [f32; 5] = [30.0, 25.0, 15.0, 15.0, 15.0];

pub fn grade(score: u8) -> &'static str {
    match score {
        90..=100 => "Excelente",
        75..=89 => "Bueno",
        55..=74 => "Mejorable",
        _ => "Necesita atención",
    }
}

/// Avance del diagnóstico en milésimas (0–1000).
pub type Progress = std::sync::atomic::AtomicU32;

pub fn run() -> Result<Report, String> {
    run_with_progress(&Progress::new(0))
}

pub fn run_with_progress(progress: &Progress) -> Result<Report, String> {
    let raw = plat::run_script(progress)?;
    progress.store(950, std::sync::atomic::Ordering::Relaxed);
    let (power_guid, power_name) = active_power_plan();
    let mem = sys::memory();
    let (disk_free, disk_total) = sys::disk_space(&sys::system_drive());
    let mut rep = Report {
        ts: fmt::now(),
        raw,
        power_guid,
        power_name,
        ram_load: mem.load,
        commit: mem.commit_pct(),
        disk_free,
        disk_total,
        lenovo: sys::machine_model().to_uppercase().contains("LENOVO"),
        ..Default::default()
    };
    analyze(&mut rep);
    save(&rep);
    Ok(rep)
}

fn rec(v: &mut Vec<Rec>, severity: u8, area: &str, title: String, detail: String, action: Option<Action>) {
    v.push(Rec { severity, area: area.into(), title, detail, action });
}

/// Entradas de `hosts` que no vienen de herramientas conocidas (Docker, Laragon, WSL...)
/// ni apuntan al propio equipo.
pub fn unknown_hosts(hosts: &[String]) -> Vec<String> {
    const KNOWN: &[&str] = &[
        "docker.internal", "kubernetes", "laragon", ".test", ".localhost", "wsl", "localhost",
        "xampp", "wamp", "valet", "lando",
    ];
    hosts
        .iter()
        .filter(|h| {
            let l = h.to_lowercase();
            let local_target = l.starts_with("127.") || l.starts_with("::1") || l.starts_with("0.0.0.0");
            !(local_target || KNOWN.iter().any(|k| l.contains(k)))
        })
        .cloned()
        .collect()
}

pub fn analyze(rep: &mut Report) {
    let r = &rep.raw;
    let mut v: Vec<Rec> = Vec::new();

    // ── Seguridad (los textos y accesos directos dependen del sistema: ver plat::txt y plat::setting)
    let open = |k: &str| plat::setting(k).map(|t| Action::Open(t.to_string()));
    let d = &r.defender;
    if !d.available {
        rec(&mut v, plat::AV_MISSING_SEVERITY, "Seguridad", plat::txt("av_missing_title").into(), plat::txt("av_missing_detail").into(), open("antivirus"));
    } else {
        if !d.realtime || !d.enabled {
            rec(&mut v, 2, "Seguridad", "Protección en tiempo real DESACTIVADA".into(), format!("Tu equipo no se protege mientras navegas o abres archivos. Actívala en {}.", plat::txt("security_app")), open("realtime"));
        }
        if d.sig_age > 7 {
            rec(&mut v, 1, "Seguridad", format!("Firmas del antivirus con {} días", d.sig_age), format!("Se actualizan con {} cuando hay internet.", plat::txt("updater")), open("updates"));
        }
        if d.quick_age > 14 && plat::quick_scan_available() {
            rec(&mut v, 1, "Seguridad", format!("Hace {} días del último análisis", d.quick_age), "Un análisis rápido tarda pocos minutos y se hace en segundo plano.".into(), Some(Action::QuickScan));
        }
    }
    let off: Vec<&str> = r.firewall.iter().filter(|f| !f.enabled).map(|f| f.name.as_str()).collect();
    if !off.is_empty() {
        rec(&mut v, 2, "Seguridad", format!("Firewall desactivado en: {}", off.join(", ")), "El firewall bloquea conexiones no deseadas. Actívalo.".into(), open("firewall"));
    }
    if r.uac == Some(false) {
        rec(&mut v, 2, "Seguridad", plat::txt("uac_off").into(), plat::txt("uac_detail").into(), open("uac"));
    }
    if r.rdp {
        rec(&mut v, 1, "Seguridad", plat::txt("remote_on").into(), "Si no lo usas, desactívalo: es una puerta de entrada habitual.".into(), open("remote"));
    }
    if !r.proxy.is_empty() {
        rec(&mut v, 1, "Seguridad", format!("Proxy configurado: {}", r.proxy), "Si no lo pusiste tú, alguien podría estar viendo tu navegación.".into(), open("proxy"));
    }
    let unknown_hosts = unknown_hosts(&r.hosts);
    if !unknown_hosts.is_empty() {
        rec(
            &mut v,
            1,
            "Seguridad",
            format!("El archivo hosts tiene {} entradas desconocidas", unknown_hosts.len()),
            format!("Pueden redirigir páginas a otro sitio. Ejemplo: {}", unknown_hosts[0]),
            None,
        );
    }
    if r.encryption == 2 {
        rec(&mut v, 0, "Seguridad", "El disco no está cifrado".into(), format!("El cifrado ({}) protege tus datos si pierdes la laptop.", plat::txt("encryption")), open("encryption"));
    }
    if r.last_update_days > 45 {
        rec(&mut v, 1, "Seguridad", format!("Sin actualizaciones desde hace {} días", r.last_update_days), "Las actualizaciones cierran fallos de seguridad.".into(), open("updates"));
    }
    for s in r.startup.iter().filter(|s| s.enabled && !s.is_nexo()) {
        if s.suspicious {
            rec(&mut v, 2, "Seguridad", format!("Programa de inicio sospechoso: {}", s.name), format!("Arranca desde una carpeta poco habitual: {}. Si no lo reconoces, desactívalo.", s.exe), None);
        } else if plat::CHECK_SIGNATURES && !s.signed() && !s.missing() {
            rec(&mut v, 1, "Seguridad", format!("Programa de inicio sin firma digital: {}", s.name), "No se puede verificar quién lo creó. Si no lo reconoces, desactívalo.".into(), None);
        }
    }
    for t in &r.tasks {
        rec(&mut v, 2, "Seguridad", format!("Tarea programada sospechosa: {}", t.name), format!("{}: {}", t.why, t.exe), open("tasks"));
    }

    // ── Almacenamiento
    let free_pct = if rep.disk_total > 0 { rep.disk_free as f64 / rep.disk_total as f64 * 100.0 } else { 100.0 };
    if free_pct < 15.0 {
        rec(&mut v, 2, "Almacenamiento", "Disco casi lleno".into(), format!("Quedan {} ({:.0} %). Con menos del 15 % libre Windows se vuelve lento.", fmt::bytes(rep.disk_free), free_pct), Some(Action::GoClean));
    } else if free_pct < 25.0 {
        rec(&mut v, 1, "Almacenamiento", format!("Espacio libre justo: {:.0} %", free_pct), format!("Quedan {}. Una limpieza te dará margen.", fmt::bytes(rep.disk_free)), Some(Action::GoClean));
    }
    for dk in &r.disks {
        if !dk.health.is_empty() && dk.health != "Healthy" {
            rec(&mut v, 2, "Almacenamiento", format!("El disco {} reporta problemas", dk.name), "Haz copia de seguridad de tus videos y documentos cuanto antes.".into(), None);
        }
        if let Some(wear) = dk.wear {
            if wear >= 80 {
                rec(&mut v, 2, "Almacenamiento", format!("Desgaste del SSD: {} %", wear), "Está cerca del final de su vida útil. Haz copias de seguridad.".into(), None);
            } else if wear >= 50 {
                rec(&mut v, 1, "Almacenamiento", format!("Desgaste del SSD: {} %", wear), "Todavía funciona bien, pero conviene vigilarlo.".into(), None);
            }
        }
    }

    // ── Memoria y rendimiento
    if rep.ram_load >= 85 {
        rec(&mut v, 1, "Memoria", format!("RAM al {} % ahora mismo", rep.ram_load), "Revisa qué programas consumen más y estabiliza.".into(), Some(Action::GoRam));
    }
    if rep.commit >= 85.0 {
        rec(&mut v, 1, "Memoria", format!("Memoria virtual al {:.0} %", rep.commit), "Si se agota, los programas pueden cerrarse solos.".into(), Some(Action::GoRam));
    }
    if r.uptime_days > 7.0 {
        rec(&mut v, 1, "Memoria", format!("Llevas {:.0} días sin reiniciar", r.uptime_days), format!("Reiniciar libera memoria y termina de instalar actualizaciones.{}", plat::txt("reboot_note")), None);
    }
    if !rep.power_guid.is_empty() && rep.power_guid != BALANCED {
        rec(&mut v, 1, "Memoria", format!("Plan de energía: {}", rep.power_name), "«Equilibrado» es el recomendado: rinde al máximo cuando hace falta y ahorra batería el resto del tiempo.".into(), Some(Action::SetBalanced));
    }

    // ── Arranque
    let enabled: Vec<&StartupItem> = r.startup.iter().filter(|s| s.enabled).collect();
    if enabled.len() > 8 {
        rec(&mut v, 1, "Arranque", format!("{} programas arrancan con {}", enabled.len(), plat::txt("os")), "Desactiva los que no uses a diario: el encendido será más rápido.".into(), None);
    }
    for s in r.startup.iter().filter(|s| s.enabled && s.missing()) {
        rec(&mut v, 0, "Arranque", format!("Entrada de inicio rota: {}", s.name), "Apunta a un programa que ya no existe. Puedes desactivarla sin riesgo.".into(), None);
    }

    // ── Batería
    if let Some(h) = r.battery.health() {
        if h < 60.0 {
            rec(&mut v, 2, "Batería", format!("Batería muy desgastada: {:.0} % de su capacidad", h), "Dura bastante menos que nueva. Si se apaga sola, conviene cambiarla.".into(), Some(Action::BatteryReport));
        } else if h < 80.0 {
            rec(&mut v, 1, "Batería", format!("Batería con desgaste: {:.0} % de su capacidad", h), "Normal con el uso. Evita tenerla siempre al 100 % y con calor.".into(), Some(Action::BatteryReport));
        }
        if rep.lenovo {
            rec(&mut v, 0, "Batería", "Activa el modo conservación de Lenovo".into(), plat::txt("lenovo_detail").into(), open("lenovo"));
        }
    }

    // ── Notas por área: 100 menos penalizaciones; la batería parte de su salud real.
    let mut areas = Vec::new();
    for a in AREAS {
        // Los consejos (severidad 0) son sugerencias: no restan puntos.
        let penalty: i32 = v.iter().filter(|x| x.area == a).map(|x| [0, 15, 35][x.severity as usize]).sum();
        let base: i32 = match a {
            "Batería" => match r.battery.health() {
                Some(h) => h as i32,
                None => {
                    areas.push((a.to_string(), None));
                    continue;
                }
            },
            "Arranque" => 100 - (enabled.len() as i32 - 5).max(0) * 4,
            _ => 100,
        };
        areas.push((a.to_string(), Some((base - penalty).clamp(0, 100) as u8)));
    }
    let (mut sum, mut wsum) = (0.0, 0.0);
    for (i, (_, s)) in areas.iter().enumerate() {
        if let Some(s) = s {
            sum += *s as f32 * WEIGHTS[i];
            wsum += WEIGHTS[i];
        }
    }
    v.sort_by_key(|x| std::cmp::Reverse(x.severity));
    rep.overall = if wsum > 0.0 { (sum / wsum).round() as u8 } else { 0 };
    rep.areas = areas;
    rep.recs = v;
}

fn path() -> std::path::PathBuf {
    config::data_dir().join("diagnostico.json")
}

pub fn save(rep: &Report) {
    let _ = std::fs::create_dir_all(config::data_dir());
    if let Ok(s) = serde_json::to_string(rep) {
        let _ = std::fs::write(path(), s);
    }
}

pub fn load() -> Option<Report> {
    std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok())
}
