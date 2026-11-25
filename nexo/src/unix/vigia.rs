// ════════════════════════════════════════════════════════════════════════
//  NEXO · nexo/src/unix/vigia.rs
//  Autor: Chatta Betanzos, Jimy Alexander  ·  jimy-betanzos
// ════════════════════════════════════════════════════════════════════════

//! Vigía en Linux y macOS: proceso mínimo en segundo plano (sin ventana) que cada 30 s
//! revisa RAM, memoria de intercambio, CPU, fugas de memoria, disco y programas nuevos al
//! inicio, y avisa con notificaciones del sistema. Nunca cierra nada por su cuenta.
//!
//! A diferencia de Windows, no muestra un icono junto al reloj (eso exigiría bibliotecas
//! gráficas distintas en cada escritorio de Linux): se controla desde NEXO › Memoria RAM.

// jimy-betanzos · dependencia: biblioteca estándar de Rust (collections)
use std::collections::{HashMap, HashSet, VecDeque};
// jimy-betanzos · dependencia: biblioteca estándar de Rust (path)
use std::path::PathBuf;
// jimy-betanzos · dependencia: biblioteca estándar de Rust (time)
use std::time::{Duration, Instant, SystemTime};

// jimy-betanzos · dependencia: módulo propio config
use crate::config::{self, JbConfig};
// jimy-betanzos · dependencia: módulo propio procs
use crate::procs::{JbAppGroup, JbSampler};
// jimy-betanzos · dependencia: módulo propio
use crate::{fmt, install, notify, sys};

const TICK: Duration = Duration::from_secs(30);
const LEAK_SAMPLES: usize = 20; // 20 × 30 s = 10 min
const LEAK_BYTES: u64 = 1 << 30;

/// jimy-betanzos · Archivo donde el vigía guarda su número de proceso.
fn jb_pid_file() -> PathBuf {
    config::jb_data_dir().join("vigia.pid")
}

/// jimy-betanzos · Indica si un proceso sigue vivo.
fn jb_alive(pid: i32) -> bool {
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

/// jimy-betanzos · Lee el número de proceso del vigía.
fn jb_read_pid() -> Option<i32> {
    std::fs::read_to_string(jb_pid_file()).ok()?.trim().parse().ok().filter(|&p| jb_alive(p))
}

/// jimy-betanzos · Indica si el vigía está en marcha.
pub fn jb_is_running() -> bool {
    jb_read_pid().is_some()
}

/// jimy-betanzos · Arranca el vigía en segundo plano.
pub fn jb_start() -> bool {
    std::process::Command::new(install::jb_installed_exe())
        .arg("--vigia")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// jimy-betanzos · Detiene el vigía y espera a que termine. Devuelve si estaba activo.
pub fn jb_stop_and_wait() -> bool {
    let Some(pid) = jb_read_pid() else { return false };
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }
    for _ in 0..40 {
        if !jb_alive(pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_file(jb_pid_file());
    true
}

/// jimy-betanzos · Texto con los programas que más memoria usan.
fn jb_top_consumers(groups: &[JbAppGroup], n: usize) -> String {
    groups
        .iter()
        .filter(|g| !g.system)
        .take(n)
        .map(|g| format!("{} {}", g.name, fmt::jb_bytes(g.private)))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// jimy-betanzos · Estado del vigía entre una revisión y la siguiente.
struct JbState {
    sampler: JbSampler,
    cfg: JbConfig,
    cfg_mtime: Option<SystemTime>,
    last_alert: HashMap<String, Instant>,
    high: u32,
    crit: u32,
    cpu_hi: u32,
    hist: HashMap<String, VecDeque<u64>>,
    startup: Option<HashSet<String>>,
}

/// jimy-betanzos · Métodos de State.
impl JbState {
    /// jimy-betanzos · Envía un aviso si no se repitió hace poco.
    fn jb_alert(&mut self, key: &str, cooldown: Duration, title: &str, text: &str) {
        if self.last_alert.get(key).is_some_and(|t| t.elapsed() < cooldown) {
            return;
        }
        self.last_alert.insert(key.to_string(), Instant::now());
        notify::jb_toast(title, text, "");
        config::jb_push_alert(key.split(':').next().unwrap_or(key), &format!("{}: {}", title, text));
    }

    /// jimy-betanzos · Revisión periódica de memoria, CPU y disco.
    fn jb_tick(&mut self) {
        let mtime = config::jb_config_mtime();
        if mtime != self.cfg_mtime {
            self.cfg = JbConfig::jb_load();
            self.cfg_mtime = mtime;
        }
        let mem = sys::jb_memory();
        let (groups, cpu) = self.sampler.jb_sample(false);

        // Fugas: un programa que crece sin parar durante 10 minutos.
        if self.cfg.alert_leaks {
            let mut seen = HashSet::new();
            let mut leak: Option<(String, u64)> = None;
            for g in groups.iter().filter(|g| !g.system).take(40) {
                seen.insert(g.exe.clone());
                let h = self.hist.entry(g.exe.clone()).or_default();
                h.push_back(g.private);
                if h.len() > LEAK_SAMPLES {
                    h.pop_front();
                }
                if h.len() == LEAK_SAMPLES {
                    let (first, last) = (h[0], h[LEAK_SAMPLES - 1]);
                    let drops = h.iter().zip(h.iter().skip(1)).filter(|(a, b)| b < a).count();
                    if last > first + LEAK_BYTES && drops <= 3 {
                        leak = Some((g.name.clone(), last - first));
                    }
                }
            }
            self.hist.retain(|k, _| seen.contains(k));
            if let Some((name, grow)) = leak {
                self.jb_alert(
                    &format!("fuga:{}", name),
                    Duration::from_secs(3600),
                    "Posible fuga de memoria",
                    &format!("{} creció {} en 10 min sin parar. Si va lento, guarda tu trabajo y reinícialo.", name, fmt::jb_bytes(grow)),
                );
            }
        }

        // RAM: dos lecturas seguidas (1 min) por encima del umbral para no avisar por picos.
        // Aquí no se estabiliza solo: liberar la caché del sistema pide la contraseña.
        let load = mem.load.min(100) as u8;
        self.crit = if load >= self.cfg.ram_critical { self.crit + 1 } else { 0 };
        self.high = if load >= self.cfg.ram_warn { self.high + 1 } else { 0 };
        if self.crit >= 2 {
            self.jb_alert(
                "critica",
                Duration::from_secs(900),
                "Memoria crítica",
                &format!("RAM al {} %. Más consumo: {}. Abre NEXO › Memoria RAM para estabilizar o cerrar algo.", load, jb_top_consumers(&groups, 3)),
            );
        } else if self.high >= 2 {
            self.jb_alert("ram", Duration::from_secs(1200), "Memoria alta", &format!("RAM al {} %. Más consumo: {}.", load, jb_top_consumers(&groups, 3)));
        }

        if mem.jb_commit_pct() >= 90.0 {
            self.jb_alert(
                "virtual",
                Duration::from_secs(900),
                "Memoria de intercambio casi agotada",
                &format!("Al {:.0} %. Guarda tu trabajo: los programas podrían fallar. Más consumo: {}", mem.jb_commit_pct(), jb_top_consumers(&groups, 2)),
            );
        }

        self.cpu_hi = if cpu >= 90.0 { self.cpu_hi + 1 } else { 0 };
        if self.cfg.alert_cpu && self.cpu_hi >= 6 {
            if let Some(g) = groups.iter().max_by(|a, b| a.cpu_pct.total_cmp(&b.cpu_pct)) {
                let text = format!("Lleva 3 minutos por encima del 90 %. Principal responsable: {} ({:.0} %).", g.name, g.cpu_pct);
                self.jb_alert("cpu", Duration::from_secs(1800), "Procesador al máximo", &text);
            }
        }

        if self.cfg.alert_disk {
            let (free, total) = sys::jb_disk_space(&sys::jb_system_drive());
            if total > 0 && (free as f64 / total as f64) < 0.10 {
                self.jb_alert(
                    "disco",
                    Duration::from_secs(12 * 3600),
                    "Disco casi lleno",
                    &format!("Quedan {} libres. Abre NEXO › Limpieza para recuperar espacio.", fmt::jb_bytes(free)),
                );
            }
        }

        if self.cfg.alert_security {
            let now_entries = sys::jb_startup_entries();
            if let Some(prev) = &self.startup {
                let new: Vec<String> = now_entries.difference(prev).cloned().collect();
                if let Some(first) = new.first() {
                    let more = if new.len() > 1 { format!(" (y {} más)", new.len() - 1) } else { String::new() };
                    self.jb_alert(
                        &format!("inicio:{}", first),
                        Duration::from_secs(24 * 3600),
                        "Nuevo programa al inicio de sesión",
                        &format!("{}{}. Si no lo instalaste tú, revísalo en NEXO › Protección.", first, more),
                    );
                }
            }
            self.startup = Some(now_entries);
        }

        sys::jb_trim_own_memory();
    }
}

/// jimy-betanzos · Bucle principal del vigía.
pub fn jb_run() {
    if jb_is_running() {
        return;
    }
    let _ = std::fs::create_dir_all(config::jb_data_dir());
    let _ = std::fs::write(jb_pid_file(), std::process::id().to_string());
    sys::jb_background_process();
    let mut st = JbState {
        sampler: JbSampler::default(),
        cfg: JbConfig::jb_load(),
        cfg_mtime: config::jb_config_mtime(),
        last_alert: HashMap::new(),
        high: 0,
        crit: 0,
        cpu_hi: 0,
        hist: HashMap::new(),
        startup: None,
    };
    loop {
        st.jb_tick();
        std::thread::sleep(TICK);
    }
}
