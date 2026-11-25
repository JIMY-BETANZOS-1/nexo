// ════════════════════════════════════════════════════════════════════════
//  NEXO · nexo/src/unix/diag.rs
//  Autor: Chatta Betanzos, Jimy Alexander  ·  jimy-betanzos
// ════════════════════════════════════════════════════════════════════════

//! Parte de Linux y macOS del diagnóstico. Recoge los mismos datos que el script de
//! PowerShell de Windows, en solo lectura, leyendo archivos del sistema y con comandos
//! estándar (lsblk, systemctl, ss, fdesetup, socketfilterfw, ioreg, pmset…). Cada parte
//! informa su avance; si un comando no existe, esa comprobación queda «sin datos».

// jimy-betanzos · dependencia: biblioteca estándar de Rust (path)
use std::path::{Path, PathBuf};
// jimy-betanzos · dependencia: biblioteca estándar de Rust (process)
use std::process::Command;
// jimy-betanzos · dependencia: biblioteca estándar de Rust (sync)
use std::sync::atomic::Ordering;

// jimy-betanzos · dependencia: módulo propio de la interfaz
use super::{JbBattery, JbDefender, JbDisk, JbFirewall, JbProgress, JbRaw, JbStartupItem, JbTask};

/// Plan de energía recomendado (power-profiles-daemon en Linux).
pub const BALANCED: &str = "balanced";
/// En Linux de escritorio es normal no tener antivirus: es un consejo, no un problema.
#[cfg(not(target_os = "macos"))]
pub const AV_MISSING_SEVERITY: u8 = 0;
#[cfg(target_os = "macos")]
pub const AV_MISSING_SEVERITY: u8 = 1;
/// Linux y macOS no firman los programas de inicio como Windows (Authenticode).
pub const CHECK_SIGNATURES: bool = false;

/// jimy-betanzos · Carpeta personal del usuario.
fn jb_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// jimy-betanzos · Ejecuta un comando y devuelve su salida.
fn jb_run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr))
}

/// jimy-betanzos · Indica si un comando está instalado.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn jb_exists_cmd(cmd: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {} >/dev/null 2>&1", cmd)]).status().map(|s| s.success()).unwrap_or(false)
}

/// jimy-betanzos · Días desde la última modificación de un archivo.
fn jb_days_since(p: &Path) -> Option<i64> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    Some((std::time::SystemTime::now().duration_since(m).ok()?.as_secs() / 86400) as i64)
}

/// jimy-betanzos · Indica si un programa se ejecuta desde una carpeta sospechosa.
fn jb_suspicious_path(p: &str) -> bool {
    let h = jb_home().to_string_lossy().to_string();
    ["/tmp/", "/var/tmp/", "/dev/shm/", "/private/tmp/", "/Users/Shared/"].iter().any(|d| p.starts_with(d))
        || p.starts_with(&format!("{}/.cache/", h))
        || p.starts_with(&format!("{}/Library/Caches/", h))
        || p.starts_with(&format!("{}/Downloads/", h))
}

// ───────────────────────── partes del diagnóstico ─────────────────────────

/// jimy-betanzos · Programas que se abren al iniciar sesión.
#[cfg(not(target_os = "macos"))]
fn jb_startup() -> Vec<JbStartupItem> {
    let mut out = Vec::new();
    for (dir, scope) in [(jb_home().join(".config/autostart"), "Usuario"), (PathBuf::from("/etc/xdg/autostart"), "Todos los usuarios")] {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("desktop") {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{}=", k))).unwrap_or("").trim().to_string();
            let exec = get("Exec");
            let exe = exec.trim_matches('"').split_whitespace().next().unwrap_or("").trim_matches('"').to_string();
            // Una copia en ~/.config/autostart con Hidden=true desactiva la de /etc/xdg.
            let user_copy = jb_home().join(".config/autostart").join(e.file_name());
            let hidden = |t: &str| t.lines().any(|l| l.trim() == "Hidden=true" || l.trim() == "X-GNOME-Autostart-enabled=false");
            let enabled = !hidden(&text) && !(scope != "Usuario" && std::fs::read_to_string(&user_copy).map(|t| hidden(&t)).unwrap_or(false));
            let name = if get("Name").is_empty() { e.file_name().to_string_lossy().to_string() } else { get("Name") };
            out.push(JbStartupItem {
                name,
                command: exec.clone(),
                exe: exe.clone(),
                hive: if scope == "Usuario" { "HKCU".into() } else { "HKLM".into() },
                sub: p.display().to_string(),
                scope: scope.into(),
                enabled,
                signature: if exe.is_empty() || exe.starts_with('/') && !Path::new(&exe).exists() { "missing".into() } else { "n/a".into() },
                publisher: String::new(),
                suspicious: jb_suspicious_path(&exe),
            });
        }
    }
    out
}

/// jimy-betanzos · Programas que se abren al iniciar sesión.
#[cfg(target_os = "macos")]
fn jb_startup() -> Vec<JbStartupItem> {
    let mut out = Vec::new();
    let user_dir = jb_home().join("Library/LaunchAgents");
    let off_dir = crate::config::jb_data_dir().join("inicio_desactivado");
    let dirs = [(user_dir, "Usuario", true), (off_dir, "Usuario", false), (PathBuf::from("/Library/LaunchAgents"), "Todos los usuarios", true), (PathBuf::from("/Library/LaunchDaemons"), "Sistema", true)];
    for (dir, scope, on) in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("plist") {
                continue;
            }
            let ps = p.to_string_lossy().to_string();
            let exe = jb_run("plutil", &["-extract", "ProgramArguments.0", "raw", &ps])
                .filter(|s| s.starts_with('/'))
                .or_else(|| jb_run("plutil", &["-extract", "Program", "raw", &ps]).filter(|s| s.starts_with('/')))
                .unwrap_or_default()
                .trim()
                .to_string();
            let disabled = jb_run("plutil", &["-extract", "Disabled", "raw", &ps]).map(|s| s.trim() == "true").unwrap_or(false);
            out.push(JbStartupItem {
                name: p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                command: exe.clone(),
                exe: exe.clone(),
                hive: if scope == "Usuario" { "HKCU".into() } else { "HKLM".into() },
                sub: ps.clone(),
                scope: scope.into(),
                enabled: on && !disabled,
                signature: if !exe.is_empty() && !Path::new(&exe).exists() { "missing".into() } else { "n/a".into() },
                publisher: String::new(),
                suspicious: jb_suspicious_path(&exe),
            });
        }
    }
    out
}

/// jimy-betanzos · Discos del equipo con su espacio.
fn jb_disks() -> Vec<JbDisk> {
    #[cfg(target_os = "macos")]
    {
        let info = jb_run("diskutil", &["info", "/"]).unwrap_or_default();
        let field = |k: &str| info.lines().find_map(|l| l.trim().strip_prefix(k).map(|v| v.trim().to_string())).unwrap_or_default();
        let smart = field("SMART Status:");
        vec![JbDisk {
            name: field("Device / Media Name:"),
            media: if field("Solid State:") == "Yes" { "SSD".into() } else { "HDD".into() },
            health: match smart.as_str() {
                "Verified" => "Healthy".into(),
                "" | "Not Supported" => String::new(),
                other => other.into(),
            },
            ..Default::default()
        }]
    }
    #[cfg(not(target_os = "macos"))]
    {
        // NAME ROTA SIZE TYPE  →  «nvme0n1 0 512110190592 disk»
        let text = jb_run("lsblk", &["-dbno", "NAME,ROTA,SIZE,TYPE"]).unwrap_or_default();
        text.lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                (f.len() == 4 && f[3] == "disk").then(|| JbDisk {
                    name: f[0].into(),
                    media: if f[1] == "0" { "SSD".into() } else { "HDD".into() },
                    size: f[2].parse().unwrap_or(0),
                    ..Default::default()
                })
            })
            .collect()
    }
}

/// jimy-betanzos · Datos de la batería.
fn jb_battery() -> JbBattery {
    #[cfg(target_os = "macos")]
    {
        let text = jb_run("ioreg", &["-rn", "AppleSmartBattery"]).unwrap_or_default();
        let num = |k: &str| text.lines().find_map(|l| l.trim().strip_prefix(&format!("\"{}\" = ", k))?.trim().parse::<i64>().ok()).unwrap_or(0);
        let full = [num("AppleRawMaxCapacity"), num("NominalChargeCapacity"), num("MaxCapacity")].into_iter().find(|&v| v > 100).unwrap_or(0);
        JbBattery { design: num("DesignCapacity"), full, cycles: num("CycleCount") }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let Ok(rd) = std::fs::read_dir("/sys/class/power_supply") else { return JbBattery::default() };
        for e in rd.flatten() {
            let d = e.path();
            if std::fs::read_to_string(d.join("type")).map(|t| t.trim() == "Battery").unwrap_or(false) {
                let r = |f: &str| std::fs::read_to_string(d.join(f)).ok().and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
                let (design, full) = if r("energy_full_design") > 0 { (r("energy_full_design"), r("energy_full")) } else { (r("charge_full_design"), r("charge_full")) };
                return JbBattery { design: design / 1000, full: full / 1000, cycles: r("cycle_count") };
            }
        }
        JbBattery::default()
    }
}

/// jimy-betanzos · Estado del antivirus (ClamAV o XProtect).
fn jb_antivirus() -> JbDefender {
    #[cfg(target_os = "macos")]
    {
        // XProtect viene con macOS y se actualiza solo.
        let bundle = Path::new("/Library/Apple/System/Library/CoreServices/XProtect.bundle");
        JbDefender { available: true, enabled: true, realtime: true, sig_age: jb_days_since(bundle).unwrap_or(0), quick_age: 0, tamper: true }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let available = jb_exists_cmd("clamscan") || jb_exists_cmd("clamd");
        let realtime = crate::sys::jb_running_processes().iter().any(|p| p == "clamd" || p == "clamonacc");
        let db = ["/var/lib/clamav/daily.cld", "/var/lib/clamav/daily.cvd"].iter().find_map(|p| jb_days_since(Path::new(p)));
        let scan = crate::config::jb_data_dir().join("ultimo_analisis");
        JbDefender { available, enabled: available, realtime: available && realtime, sig_age: db.unwrap_or(0), quick_age: jb_days_since(&scan).unwrap_or(10000), tamper: false }
    }
}

/// jimy-betanzos · Estado del firewall.
fn jb_firewall() -> Vec<JbFirewall> {
    #[cfg(target_os = "macos")]
    {
        let s = jb_run("/usr/libexec/ApplicationFirewall/socketfilterfw", &["--getglobalstate"]).unwrap_or_default().to_lowercase();
        if s.is_empty() {
            return vec![];
        }
        vec![JbFirewall { name: "Firewall de macOS".into(), enabled: s.contains("enabled") || s.contains("activad") }]
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(conf) = std::fs::read_to_string("/etc/ufw/ufw.conf") {
            return vec![JbFirewall { name: "ufw".into(), enabled: conf.lines().any(|l| l.trim() == "ENABLED=yes") }];
        }
        if jb_exists_cmd("firewall-cmd") || Path::new("/etc/firewalld").exists() {
            let active = jb_run("systemctl", &["is-active", "firewalld"]).map(|s| s.trim() == "active").unwrap_or(false);
            return vec![JbFirewall { name: "firewalld".into(), enabled: active }];
        }
        vec![JbFirewall { name: "Firewall (ufw o firewalld no instalado)".into(), enabled: false }]
    }
}

/// jimy-betanzos · macOS: Gatekeeper (equivalente a «preguntar antes de abrir apps desconocidas»).
fn jb_gatekeeper() -> Option<bool> {
    #[cfg(target_os = "macos")]
    return jb_run("spctl", &["--status"]).map(|s| s.contains("enabled"));
    #[cfg(not(target_os = "macos"))]
    None
}

/// jimy-betanzos · Servidor SSH escuchando (acceso remoto al equipo).
fn jb_remote_access() -> bool {
    #[cfg(target_os = "macos")]
    let text = jb_run("netstat", &["-an", "-p", "tcp"]).unwrap_or_default();
    #[cfg(not(target_os = "macos"))]
    let text = jb_run("ss", &["-ltn"]).unwrap_or_default();
    text.lines().any(|l| (l.contains(":22 ") || l.contains(".22 ")) && l.to_uppercase().contains("LISTEN"))
}

/// jimy-betanzos · Configuración de proxy del sistema.
fn jb_proxy() -> String {
    for k in ["https_proxy", "HTTPS_PROXY", "http_proxy", "HTTP_PROXY"] {
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let s = jb_run("scutil", &["--proxy"]).unwrap_or_default();
        if s.contains("HTTPEnable : 1") || s.contains("HTTPSEnable : 1") {
            return s.lines().find_map(|l| l.trim().strip_prefix("HTTPProxy : ").map(String::from)).unwrap_or_else(|| "manual".into());
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mode = jb_run("gsettings", &["get", "org.gnome.system.proxy", "mode"]).unwrap_or_default();
        if mode.contains("manual") || mode.contains("auto") {
            return format!("GNOME ({})", mode.trim().trim_matches('\''));
        }
    }
    String::new()
}

/// jimy-betanzos · Entradas extra del archivo hosts.
fn jb_hosts() -> Vec<String> {
    // El nombre del propio equipo también es una entrada normal.
    let me = std::fs::read_to_string("/etc/hostname").ok().or_else(|| jb_run("hostname", &[])).unwrap_or_default().trim().to_lowercase();
    std::fs::read_to_string("/etc/hosts")
        .unwrap_or_default()
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty())
        .filter(|l| {
            let low = l.to_lowercase();
            if !me.is_empty() && low.split_whitespace().skip(1).any(|n| n == me || n.starts_with(&format!("{}.", me))) {
                return false;
            }
            // Entradas estándar del sistema
            !(low.ends_with(" localhost") || low.contains("ip6-") || low.contains("broadcasthost") || low.starts_with("::1") || low.starts_with("127.0.1.1") || low.starts_with("fe00::") || low.starts_with("ff0"))
        })
        .collect()
}

/// jimy-betanzos · 1 cifrado · 2 sin cifrar · 0 desconocido.
fn jb_encryption() -> i64 {
    #[cfg(target_os = "macos")]
    {
        let s = jb_run("fdesetup", &["status"]).unwrap_or_default();
        if s.contains("is On") { 1 } else if s.contains("is Off") { 2 } else { 0 }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let s = jb_run("lsblk", &["-no", "TYPE"]).unwrap_or_default();
        if s.lines().any(|l| l.trim() == "crypt") { 1 } else if s.is_empty() { 0 } else { 2 }
    }
}

/// jimy-betanzos · Días desde la última actualización del sistema.
fn jb_last_update_days() -> i64 {
    #[cfg(target_os = "macos")]
    let files = ["/Library/Receipts/InstallHistory.plist"];
    #[cfg(not(target_os = "macos"))]
    let files = ["/var/log/apt/history.log", "/var/log/dnf.log", "/var/log/pacman.log", "/var/lib/apt/periodic/update-success-stamp"];
    files.iter().filter_map(|f| jb_days_since(Path::new(f))).min().unwrap_or(-1)
}

/// jimy-betanzos · Días que el equipo lleva encendido.
fn jb_uptime_days() -> f64 {
    #[cfg(target_os = "macos")]
    {
        // «{ sec = 1700000000, usec = 0 } Mon Nov …»
        let s = jb_run("sysctl", &["-n", "kern.boottime"]).unwrap_or_default();
        let boot: u64 = s.split("sec = ").nth(1).and_then(|r| r.split(',').next()).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        if boot == 0 { 0.0 } else { crate::fmt::jb_now().saturating_sub(boot) as f64 / 86400.0 }
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::fs::read_to_string("/proc/uptime").ok().and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok()).map(|s| s / 86400.0).unwrap_or(0.0)
    }
}

/// jimy-betanzos · Nombre y versión del sistema operativo.
fn jb_os_name() -> String {
    #[cfg(target_os = "macos")]
    {
        format!("macOS {}", jb_run("sw_vers", &["-productVersion"]).unwrap_or_default().trim())
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::fs::read_to_string("/etc/os-release")
            .unwrap_or_default()
            .lines()
            .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()))
            .unwrap_or_else(|| "Linux".into())
    }
}

/// jimy-betanzos · Tareas programadas del usuario (cron) que ejecutan algo desde carpetas temporales.
fn jb_tasks() -> (Vec<JbTask>, i64) {
    let text = jb_run("crontab", &["-l"]).unwrap_or_default();
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    let bad = lines
        .iter()
        .filter_map(|l| {
            let exe = l.split_whitespace().skip(5).find(|w| w.starts_with('/') || w.starts_with("\"/"))?.trim_matches('"').to_string();
            jb_suspicious_path(&exe).then(|| JbTask { name: "cron".into(), path: "crontab".into(), exe, why: "Se ejecuta desde una carpeta temporal o de descargas".into() })
        })
        .collect();
    (bad, lines.len() as i64)
}

/// jimy-betanzos · Recoge todos los datos del diagnóstico (solo lectura).
pub fn jb_run_script(progress: &JbProgress) -> Result<JbRaw, String> {
    let step = |i: u32| progress.store(60 + 860 * i / 8, Ordering::Relaxed);
    let mut raw = JbRaw { startup: jb_startup(), ..Default::default() };
    step(1);
    raw.disks = jb_disks();
    step(2);
    raw.battery = jb_battery();
    step(3);
    raw.defender = jb_antivirus();
    step(4);
    raw.firewall = jb_firewall();
    raw.uac = jb_gatekeeper();
    step(5);
    raw.rdp = jb_remote_access();
    raw.proxy = jb_proxy();
    raw.hosts = jb_hosts();
    step(6);
    raw.encryption = jb_encryption();
    raw.last_update_days = jb_last_update_days();
    step(7);
    raw.uptime_days = jb_uptime_days();
    raw.os = jb_os_name();
    (raw.tasks, raw.tasks_total) = jb_tasks();
    step(8);
    Ok(raw)
}

// ───────────────────────── acciones ─────────────────────────

/// jimy-betanzos · (identificador, nombre) del perfil de energía activo. Solo Linux con power-profiles-daemon.
pub fn jb_active_power_plan() -> (String, String) {
    #[cfg(target_os = "macos")]
    return (String::new(), String::new());
    #[cfg(not(target_os = "macos"))]
    {
        let Some(p) = jb_run("powerprofilesctl", &["get"]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && !s.contains(' ')) else {
            return (String::new(), String::new());
        };
        let name = match p.as_str() {
            "balanced" => "Equilibrado",
            "performance" => "Alto rendimiento",
            "power-saver" => "Ahorro de energía",
            other => other,
        };
        (p.clone(), name.to_string())
    }
}

/// jimy-betanzos · Cambia el perfil de energía.
pub fn jb_set_power_plan(id: &str) -> bool {
    Command::new("powerprofilesctl").args(["set", id]).status().map(|s| s.success()).unwrap_or(false)
}

/// jimy-betanzos · Activa o desactiva un programa de inicio sin borrarlo (se puede volver a activar).
pub fn jb_set_startup_enabled(item: &JbStartupItem, on: bool) -> Result<(), String> {
    if item.jb_machine_wide() && cfg!(target_os = "macos") {
        return Err("Este programa es de todo el equipo: desactívalo en Ajustes del Sistema › General › Ítems de inicio.".into());
    }
    let src = PathBuf::from(&item.sub);
    #[cfg(target_os = "macos")]
    {
        // Se mueve el LaunchAgent a la carpeta de NEXO (desactivado) o de vuelta.
        let off_dir = crate::config::jb_data_dir().join("inicio_desactivado");
        let user_dir = jb_home().join("Library/LaunchAgents");
        let name = src.file_name().ok_or("Ruta no válida")?;
        let (from, to) = if on { (off_dir.join(name), user_dir.join(name)) } else { (src.clone(), off_dir.join(name)) };
        let _ = std::fs::create_dir_all(to.parent().unwrap_or(&to));
        if !on {
            let _ = Command::new("launchctl").arg("unload").arg(&from).output();
        }
        std::fs::rename(&from, &to).map_err(|e| e.to_string())?;
        if on {
            let _ = Command::new("launchctl").arg("load").arg(&to).output();
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Estándar freedesktop: una copia en ~/.config/autostart con Hidden=true lo desactiva.
        let user_dir = jb_home().join(".config/autostart");
        std::fs::create_dir_all(&user_dir).map_err(|e| e.to_string())?;
        let dest = user_dir.join(src.file_name().ok_or("Ruta no válida")?);
        let text = std::fs::read_to_string(if dest.exists() { &dest } else { &src }).map_err(|e| e.to_string())?;
        let mut lines: Vec<String> = text.lines().filter(|l| !l.starts_with("Hidden=") && !l.starts_with("X-GNOME-Autostart-enabled=")).map(String::from).collect();
        if !on {
            lines.push("Hidden=true".into());
        }
        std::fs::write(&dest, lines.join("\n") + "\n").map_err(|e| e.to_string())
    }
}

/// jimy-betanzos · Indica si hay un antivirus que pueda hacer un análisis rápido.
pub fn jb_quick_scan_available() -> bool {
    #[cfg(target_os = "macos")]
    return false; // XProtect analiza solo; no ofrece un análisis manual
    #[cfg(not(target_os = "macos"))]
    jb_exists_cmd("clamscan")
}

/// jimy-betanzos · Análisis rápido con ClamAV de las carpetas donde suele llegar software (Descargas y
/// Escritorio). Devuelve 0 si está limpio y 2 si encontró amenazas (como Defender).
pub fn jb_quick_scan() -> Result<i32, String> {
    let env = crate::rules::JbEnv::jb_detect();
    let mut c = Command::new("clamscan");
    c.args(["-r", "-i", "--no-summary"]);
    if let Some(d) = &env.downloads {
        c.arg(d);
    }
    let code = c.status().map_err(|e| e.to_string())?.code().unwrap_or(-1);
    let _ = std::fs::write(crate::config::jb_data_dir().join("ultimo_analisis"), crate::fmt::jb_now().to_string());
    Ok(match code {
        1 => 2,
        c => c,
    })
}

/// jimy-betanzos · Abre la información de batería del sistema.
pub fn jb_battery_report() {
    #[cfg(target_os = "macos")]
    jb_open("x-apple.systempreferences:com.apple.Battery-Settings.extension");
    #[cfg(not(target_os = "macos"))]
    {
        let _ = Command::new("gnome-power-statistics").spawn();
    }
}

/// jimy-betanzos · Abre un ajuste del sistema o una dirección.
pub fn jb_open(target: &str) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(target).spawn();
    #[cfg(not(target_os = "macos"))]
    let _ = Command::new("xdg-open").arg(target).spawn();
}

// ───────────────────────── textos ─────────────────────────

/// jimy-betanzos · Textos que dependen del sistema operativo.
pub fn jb_txt(k: &str) -> &'static str {
    #[cfg(target_os = "macos")]
    let t = match k {
        "os" => "macOS",
        "av" => "XProtect",
        "security_app" => "Ajustes del Sistema › Privacidad y seguridad",
        "updater" => "las actualizaciones de macOS",
        "av_missing_title" => "No se pudo leer XProtect",
        "av_missing_detail" => "Revisa que macOS esté actualizado.",
        "uac_name" => "Gatekeeper",
        "uac_on" => "Activado: macOS revisa las apps antes de abrirlas",
        "uac_off" => "Gatekeeper desactivado",
        "uac_detail" => "macOS abre apps de cualquier origen sin revisarlas. Actívalo con «sudo spctl --master-enable».",
        "remote_name" => "Sesión remota (SSH)",
        "remote_on" => "Sesión remota (SSH) activada",
        "encryption" => "FileVault",
        "reboot_note" => "",
        "lenovo_detail" => "",
        _ => "",
    };
    #[cfg(not(target_os = "macos"))]
    let t = match k {
        "os" => "Linux",
        "av" => "ClamAV",
        "security_app" => "la configuración de ClamAV",
        "updater" => "freshclam",
        "av_missing_title" => "Sin antivirus instalado",
        "av_missing_detail" => "En Linux es opcional para uso personal. Si compartes archivos con Windows, instala ClamAV (sudo apt install clamav).",
        "uac_name" => "Contraseña para cambios del sistema",
        "uac_on" => "Linux pide tu contraseña (sudo) antes de cambios importantes",
        "uac_off" => "Cambios del sistema sin contraseña",
        "uac_detail" => "Cualquier programa podría hacer cambios de administrador.",
        "remote_name" => "Servidor SSH",
        "remote_on" => "Servidor SSH activo",
        "encryption" => "LUKS",
        "reboot_note" => "",
        "lenovo_detail" => "En Linux: activa el modo conservación de batería de Lenovo (ideapad_laptop) desde los ajustes de energía.",
        _ => "",
    };
    t
}

/// jimy-betanzos · Ajuste del sistema asociado a una comprobación.
pub fn jb_setting(k: &str) -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    return Some(match k {
        "firewall" => "x-apple.systempreferences:com.apple.Network-Settings.extension?Firewall",
        "updates" => "x-apple.systempreferences:com.apple.Software-Update-Settings.extension",
        "remote" => "x-apple.systempreferences:com.apple.Sharing-Settings.extension",
        "proxy" => "x-apple.systempreferences:com.apple.Network-Settings.extension",
        "encryption" => "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension",
        "uac" => "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension",
        _ => return None,
    });
    #[cfg(not(target_os = "macos"))]
    {
        let _ = k;
        None
    }
}
