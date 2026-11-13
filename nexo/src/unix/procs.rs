//! Parte de Linux y macOS de `procs`: lista de procesos, CPU y cierre.
//!
//! Linux: se lee `/proc` (memoria, tiempo de CPU y lectura/escritura de disco por proceso).
//! macOS: se usa `ps` (memoria y tiempo de CPU; macOS no expone el disco por proceso).
//! Ni Linux ni macOS permiten «compactar» la memoria de otro programa como Windows: en su
//! lugar, Estabilizar libera la caché de memoria del sistema con permiso del usuario.

use std::process::Command;

use super::{AppGroup, ProcInfo, Win};

/// Compactar un programa concreto no existe en Linux ni en macOS.
pub const PER_APP_TRIM: bool = false;

/// Unidades de 100 ns por segundo (las mismas que usa Windows para el tiempo de CPU).
const HNS: f64 = 1e7;

fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(target_os = "linux")]
fn clk_tck() -> f64 {
    let v = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if v > 0 { v as f64 } else { 100.0 }
}

#[cfg(target_os = "linux")]
pub fn list_processes() -> Vec<ProcInfo> {
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as u64;
    let me = unsafe { libc::geteuid() };
    let tck = clk_tck();
    let mut out = Vec::with_capacity(400);
    let Ok(rd) = std::fs::read_dir("/proc") else { return out };
    for e in rd.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
        let dir = e.path();
        let Ok(stat) = std::fs::read_to_string(dir.join("stat")) else { continue };
        // El nombre va entre paréntesis y puede tener espacios: se corta tras el último ')'.
        let Some(close) = stat.rfind(')') else { continue };
        let name = std::fs::read_to_string(dir.join("comm")).map(|s| s.trim().to_lowercase()).unwrap_or_default();
        let f: Vec<&str> = stat[close + 2..].split_whitespace().collect();
        let ticks = f.get(11).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) + f.get(12).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        let statm = std::fs::read_to_string(dir.join("statm")).unwrap_or_default();
        let sm: Vec<u64> = statm.split_whitespace().filter_map(|v| v.parse().ok()).collect();
        let (resident, shared) = (sm.get(1).copied().unwrap_or(0), sm.get(2).copied().unwrap_or(0));
        let io = std::fs::read_to_string(dir.join("io"))
            .map(|t| {
                t.lines()
                    .filter(|l| l.starts_with("read_bytes") || l.starts_with("write_bytes"))
                    .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
                    .sum()
            })
            .unwrap_or(0);
        let owner = std::fs::metadata(&dir).map(|m| std::os::unix::fs::MetadataExt::uid(&m)).unwrap_or(u32::MAX);
        if name.is_empty() || resident == 0 {
            continue; // hilos del núcleo: no ocupan memoria de usuario
        }
        out.push(ProcInfo {
            pid,
            exe: name,
            private: resident.saturating_sub(shared) * page,
            working: resident * page,
            cpu: (ticks as f64 / tck * HNS) as u64,
            io,
            accessible: owner == me || me == 0,
        });
    }
    out
}

#[cfg(target_os = "macos")]
pub fn list_processes() -> Vec<ProcInfo> {
    let me = unsafe { libc::geteuid() };
    let mut out = Vec::new();
    // pid, uid, memoria residente (KB), tiempo de CPU acumulado, ruta del ejecutable
    let Some(text) = run("ps", &["-axo", "pid=,uid=,rss=,time=,comm="]) else { return out };
    for l in text.lines() {
        let mut it = l.split_whitespace();
        let (Some(pid), Some(uid), Some(rss), Some(time)) = (it.next(), it.next(), it.next(), it.next()) else { continue };
        let comm: Vec<&str> = it.collect();
        let path = comm.join(" ");
        let name = path.rsplit('/').next().unwrap_or(&path).to_lowercase();
        let secs = parse_cpu_time(time);
        let rss = rss.parse::<u64>().unwrap_or(0) * 1024;
        out.push(ProcInfo {
            pid: pid.parse().unwrap_or(0),
            exe: name,
            private: rss,
            working: rss,
            cpu: (secs * HNS) as u64,
            io: 0,
            accessible: uid.parse::<u32>().map(|u| u == me || me == 0).unwrap_or(false),
        });
    }
    out
}

/// "1:02.35" o "1-02:03:04" → segundos.
#[cfg(target_os = "macos")]
fn parse_cpu_time(t: &str) -> f64 {
    let (days, rest) = match t.split_once('-') {
        Some((d, r)) => (d.parse::<f64>().unwrap_or(0.0), r),
        None => (0.0, t),
    };
    let mut secs = 0.0;
    for part in rest.split(':') {
        secs = secs * 60.0 + part.parse::<f64>().unwrap_or(0.0);
    }
    days * 86400.0 + secs
}

/// Ventanas con título (solo Linux con X11 y la herramienta `wmctrl`). Sirve para detectar
/// documentos «sin guardar»; si no está disponible, NEXO usa solo la lista de editores.
pub fn top_windows() -> Vec<Win> {
    #[cfg_attr(target_os = "macos", allow(unused_mut))]
    let mut v = Vec::new();
    #[cfg(target_os = "linux")]
    if let Some(text) = run("wmctrl", &["-lp"]) {
        for l in text.lines() {
            let f: Vec<&str> = l.splitn(5, char::is_whitespace).filter(|s| !s.is_empty()).collect();
            if f.len() >= 4 {
                let hwnd = i64::from_str_radix(f[0].trim_start_matches("0x"), 16).unwrap_or(0) as isize;
                let pid = f[2].parse().unwrap_or(0);
                let title = l.split_whitespace().skip(4).collect::<Vec<_>>().join(" ");
                v.push(Win { hwnd, pid, title });
            }
        }
    }
    v
}

/// Programa en primer plano (macOS: `lsappinfo`; Linux con X11: `xdotool`).
pub fn foreground_pid() -> u32 {
    #[cfg(target_os = "macos")]
    {
        let asn = run("lsappinfo", &["front"]).unwrap_or_default();
        let info = run("lsappinfo", &["info", "-only", "pid", asn.trim()]).unwrap_or_default();
        info.rsplit('=').next().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
    }
    #[cfg(not(target_os = "macos"))]
    {
        run("xdotool", &["getactivewindow", "getwindowpid"]).and_then(|s| s.trim().parse().ok()).unwrap_or(0)
    }
}

/// (inactivo, total) acumulados del sistema en unidades de 100 ns.
pub fn cpu_times() -> Option<(u64, u64)> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        let v: Vec<u64> = stat.lines().next()?.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
        let total: u64 = v.iter().take(8).sum();
        let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
        let k = HNS / clk_tck();
        Some(((idle as f64 * k) as u64, (total as f64 * k) as u64))
    }
    #[cfg(target_os = "macos")]
    {
        // Suma del tiempo de CPU de todos los procesos frente al tiempo transcurrido × núcleos.
        let busy: u64 = list_processes().iter().map(|p| p.cpu).sum();
        let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as u64;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos() as u64 / 100;
        let total = now * ncpu;
        Some((total.saturating_sub(busy), total))
    }
}

/// No hay compactación por programa en Linux ni macOS.
pub fn trim(_g: &AppGroup) -> u64 {
    0
}

/// Estabilizar en Linux y macOS: libera la caché de memoria del sistema (archivos que el
/// sistema guardó por si acaso). No cierra programas. Pide la contraseña al usuario.
pub fn stabilize_system() -> (usize, u64) {
    let before = crate::sys::memory().avail;
    #[cfg(target_os = "macos")]
    let cmd = "purge";
    #[cfg(not(target_os = "macos"))]
    let cmd = "sync; echo 3 > /proc/sys/vm/drop_caches";
    if crate::sys::run_as_admin(cmd).is_err() {
        return (0, 0);
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    (1, crate::sys::memory().avail.saturating_sub(before))
}

/// Pide al programa que se cierre como si pulsaras «Salir»: puede preguntarte si guardar.
pub fn close_gracefully(g: &AppGroup) -> usize {
    #[cfg(target_os = "macos")]
    {
        let ok = Command::new("osascript")
            .args(["-e", &format!("tell application \"{}\" to quit", g.name.replace('"', ""))])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return g.pids.len();
        }
    }
    signal_all(g, libc::SIGTERM)
}

/// Cierre forzado: se pierde lo no guardado. Solo con doble confirmación del usuario.
pub fn force_kill(g: &AppGroup) -> usize {
    if g.system {
        return 0;
    }
    signal_all(g, libc::SIGKILL)
}

fn signal_all(g: &AppGroup, sig: i32) -> usize {
    g.pids.iter().filter(|&&pid| unsafe { libc::kill(pid as i32, sig) } == 0).count()
}
