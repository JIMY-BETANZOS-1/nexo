//! Llamadas al sistema en Linux y macOS (misma interfaz que `sys.rs` de Windows).
//!
//! En Linux se lee `/proc` y `/sys`; en macOS se usan `sysctl`, `vm_stat` y `pmset`.
//! Todo es de solo lectura salvo vaciar la Papelera.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn read(p: &str) -> Option<String> {
    std::fs::read_to_string(p).ok()
}

pub fn is_admin() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// Nombres de los programas en marcha, en minúsculas (p. ej. "firefox", "google chrome").
pub fn running_processes() -> HashSet<String> {
    let mut set = HashSet::new();
    #[cfg(target_os = "linux")]
    if let Ok(rd) = std::fs::read_dir("/proc") {
        for e in rd.flatten() {
            if let Some(name) = read(&format!("{}/comm", e.path().display())) {
                set.insert(name.trim().to_lowercase());
            }
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(text) = run("ps", &["-axco", "comm="]) {
        for l in text.lines() {
            set.insert(l.trim().to_lowercase());
        }
    }
    set
}

pub struct Power {
    pub on_ac: bool,
    pub percent: Option<u8>,
    pub has_battery: bool,
}

pub fn power() -> Power {
    #[cfg(target_os = "linux")]
    {
        let mut p = Power { on_ac: true, percent: None, has_battery: false };
        if let Ok(rd) = std::fs::read_dir("/sys/class/power_supply") {
            for e in rd.flatten() {
                let dir = e.path();
                let kind = std::fs::read_to_string(dir.join("type")).unwrap_or_default();
                match kind.trim() {
                    "Mains" => p.on_ac = std::fs::read_to_string(dir.join("online")).map(|s| s.trim() == "1").unwrap_or(true),
                    "Battery" => {
                        p.has_battery = true;
                        p.percent = std::fs::read_to_string(dir.join("capacity")).ok().and_then(|s| s.trim().parse().ok());
                    }
                    _ => {}
                }
            }
        }
        p
    }
    #[cfg(target_os = "macos")]
    {
        // "Now drawing from 'AC Power' … -InternalBattery-0 (id=…)	87%; charging"
        let text = run("pmset", &["-g", "batt"]).unwrap_or_default();
        let percent = text.split('\t').nth(1).and_then(|s| s.split('%').next()).and_then(|s| s.trim().parse().ok());
        Power { on_ac: text.contains("AC Power"), percent, has_battery: text.contains("InternalBattery") }
    }
}

/// Juego o video en pantalla completa. Linux y macOS no lo exponen de forma estándar:
/// se responde «no» y el modo automático se apoya en la batería y las instalaciones.
pub fn user_busy() -> bool {
    false
}

fn trash_dirs() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    return vec![home().join(".Trash")];
    #[cfg(not(target_os = "macos"))]
    {
        let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share"));
        vec![data.join("Trash/files"), data.join("Trash/info")]
    }
}

/// (bytes, elementos) en la Papelera del usuario.
pub fn recycle_bin() -> (u64, u64) {
    let (mut size, mut count) = (0u64, 0u64);
    let dirs = trash_dirs();
    if let Ok(rd) = std::fs::read_dir(&dirs[0]) {
        count = rd.flatten().count() as u64;
    }
    for e in walkdir::WalkDir::new(&dirs[0]).follow_links(false).into_iter().flatten() {
        if let Ok(m) = e.metadata() {
            if m.is_file() {
                size += m.len();
            }
        }
    }
    (size, count)
}

pub fn empty_recycle_bin() -> bool {
    #[cfg(target_os = "macos")]
    {
        Command::new("osascript").args(["-e", "tell application \"Finder\" to empty trash"]).status().map(|s| s.success()).unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut ok = true;
        for d in trash_dirs() {
            if let Ok(rd) = std::fs::read_dir(&d) {
                for e in rd.flatten() {
                    let p = e.path();
                    let r = if p.is_dir() && !p.is_symlink() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
                    ok &= r.is_ok();
                }
            }
        }
        ok
    }
}

/// (libre, total) en bytes del sistema de archivos que contiene `p`.
pub fn disk_space(p: &Path) -> (u64, u64) {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_bytes()) else { return (0, 0) };
    unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut s) != 0 {
            return (0, 0);
        }
        let f = s.f_frsize as u64;
        (s.f_bavail as u64 * f, s.f_blocks as u64 * f)
    }
}

pub struct Mem {
    pub load: u32,
    pub total: u64,
    pub avail: u64,
    /// RAM + intercambio (swap): si se agota, los programas fallan.
    pub commit_total: u64,
    pub commit_avail: u64,
}

impl Mem {
    pub fn commit_pct(&self) -> f32 {
        if self.commit_total == 0 {
            0.0
        } else {
            100.0 * (1.0 - self.commit_avail as f32 / self.commit_total as f32)
        }
    }
}

pub fn memory() -> Mem {
    #[cfg(target_os = "linux")]
    {
        let text = read("/proc/meminfo").unwrap_or_default();
        let kb = |k: &str| {
            text.lines()
                .find(|l| l.starts_with(k))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0)
                * 1024
        };
        let (total, avail) = (kb("MemTotal:"), kb("MemAvailable:"));
        let (st, sf) = (kb("SwapTotal:"), kb("SwapFree:"));
        let load = (avail * 100).checked_div(total).map_or(0, |p| 100 - p as u32);
        Mem { load, total, avail, commit_total: total + st, commit_avail: avail + sf }
    }
    #[cfg(target_os = "macos")]
    {
        let total: u64 = run("sysctl", &["-n", "hw.memsize"]).and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        // vm_stat: páginas libres, inactivas y purgables cuentan como disponibles.
        let vm = run("vm_stat", &[]).unwrap_or_default();
        let page: u64 = vm.split("page size of ").nth(1).and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse().ok()).unwrap_or(16384);
        let pages = |k: &str| {
            vm.lines()
                .find(|l| l.starts_with(k))
                .and_then(|l| l.split(':').nth(1))
                .and_then(|v| v.trim().trim_end_matches('.').parse::<u64>().ok())
                .unwrap_or(0)
        };
        let avail = (pages("Pages free") + pages("Pages inactive") + pages("Pages purgeable")) * page;
        // vm.swapusage: "total = 2048.00M  used = 512.00M  free = 1536.00M  (encrypted)"
        let sw = run("sysctl", &["-n", "vm.swapusage"]).unwrap_or_default();
        let mb = |k: &str| -> u64 {
            sw.split(k).nth(1).and_then(|s| s.split_whitespace().nth(1)).and_then(|v| v.trim_end_matches('M').parse::<f64>().ok()).map(|v| (v * 1048576.0) as u64).unwrap_or(0)
        };
        let load = (avail.min(total) * 100).checked_div(total).map_or(0, |p| 100 - p as u32);
        Mem { load, total, avail, commit_total: total + mb("total"), commit_avail: avail + mb("free") }
    }
}

pub fn system_drive() -> PathBuf {
    PathBuf::from("/")
}

/// Programas que arrancan al iniciar sesión (autostart de Linux o LaunchAgents de macOS).
pub fn startup_entries() -> HashSet<String> {
    let mut set = HashSet::new();
    #[cfg(target_os = "macos")]
    let dirs = vec![home().join("Library/LaunchAgents"), PathBuf::from("/Library/LaunchAgents"), PathBuf::from("/Library/LaunchDaemons")];
    #[cfg(not(target_os = "macos"))]
    let dirs = vec![home().join(".config/autostart"), PathBuf::from("/etc/xdg/autostart")];
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                set.insert(format!("{} → {}", e.file_name().to_string_lossy(), d.display()));
            }
        }
    }
    set
}

pub fn machine_model() -> String {
    #[cfg(target_os = "macos")]
    {
        let m = run("sysctl", &["-n", "hw.model"]).unwrap_or_default();
        format!("Apple {}", m.trim()).trim().to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let r = |f: &str| read(&format!("/sys/class/dmi/id/{}", f)).map(|s| s.trim().to_string()).unwrap_or_default();
        let maker = r("sys_vendor");
        let mut family = r("product_family");
        if family.is_empty() || family.eq_ignore_ascii_case("to be filled by o.e.m.") {
            family = r("product_version");
        }
        if family.is_empty() {
            family = r("product_name");
        }
        let s = format!("{} {}", maker, family).trim().to_string();
        if s.is_empty() { "Equipo Linux".into() } else { s }
    }
}

/// En Linux y macOS NEXO no se reinicia elevado: las tareas del sistema piden la
/// contraseña en el momento (pkexec u osascript).
pub fn relaunch_as_admin() -> bool {
    false
}

pub fn relaunch_as_admin_with(_params: &str) -> bool {
    false
}

/// Ejecuta un comando de shell con permisos de administrador, pidiendo la contraseña con
/// el diálogo del sistema. Devuelve la salida estándar.
pub fn run_as_admin(shell_cmd: &str) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    let out = Command::new("osascript")
        .args(["-e", &format!("do shell script \"{}\" with administrator privileges", shell_cmd.replace('\\', "\\\\").replace('"', "\\\""))])
        .output();
    #[cfg(not(target_os = "macos"))]
    let out = Command::new("pkexec").args(["sh", "-c", shell_cmd]).output();
    let out = out.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err("Se canceló o no se concedió el permiso.".into())
    }
}

pub fn interactive_thread() {}

/// Baja la prioridad del proceso: el usuario no nota el trabajo en segundo plano.
pub fn background_thread() {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
}

/// Devuelve al sistema la memoria propia que NEXO ya no usa.
pub fn trim_own_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0);
    }
}

pub fn background_process() {
    background_thread();
}

/// Unix (segundos) -> (año, mes, día, hora, minuto) en hora local.
pub fn local_time(unix: u64) -> (u16, u16, u16, u16, u16) {
    unsafe {
        let t = unix as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        ((tm.tm_year + 1900) as u16, (tm.tm_mon + 1) as u16, tm.tm_mday as u16, tm.tm_hour as u16, tm.tm_min as u16)
    }
}

/// Una sola ventana de NEXO: se bloquea un archivo de la carpeta de datos.
pub fn claim_single_instance() -> bool {
    use std::sync::OnceLock;
    static LOCK: OnceLock<std::fs::File> = OnceLock::new();
    let dir = crate::config::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let Ok(f) = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("ventana.lock")) else { return true };
    if f.try_lock().is_err() {
        return false;
    }
    let _ = LOCK.set(f);
    true
}

pub fn hidden_cmd(program: &str) -> Command {
    Command::new(program)
}

/// Abre un archivo con su programa. Los ejecutables y scripts nunca se lanzan: solo se
/// muestran en su carpeta.
pub fn open_file(p: &Path) {
    const RISKY: &[&str] = &["sh", "bash", "zsh", "command", "app", "pkg", "dmg", "run", "bin", "appimage", "deb", "rpm", "py", "pl", "jar", "desktop", "exe", "msi", "bat", "ps1"];
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if RISKY.contains(&ext.as_str()) || ext.is_empty() {
        reveal_in_explorer(p);
    } else {
        #[cfg(target_os = "macos")]
        let _ = Command::new("open").arg(p).spawn();
        #[cfg(not(target_os = "macos"))]
        let _ = Command::new("xdg-open").arg(p).spawn();
    }
}

/// Muestra el archivo en el Finder (macOS) o abre su carpeta (Linux).
pub fn reveal_in_explorer(p: &Path) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg("-R").arg(p).spawn();
    #[cfg(not(target_os = "macos"))]
    let _ = Command::new("xdg-open").arg(p.parent().unwrap_or(p)).spawn();
}
