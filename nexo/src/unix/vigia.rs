//! Vigía en Linux y macOS: proceso mínimo en segundo plano (sin ventana) que cada 30 s
//! revisa RAM, memoria de intercambio, CPU, fugas de memoria, disco y programas nuevos al
//! inicio, y avisa con notificaciones del sistema. Nunca cierra nada por su cuenta.
//!
//! A diferencia de Windows, no muestra un icono junto al reloj (eso exigiría bibliotecas
//! gráficas distintas en cada escritorio de Linux): se controla desde NEXO › Memoria RAM.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use crate::config::{self, Config};
use crate::procs::{AppGroup, Sampler};
use crate::{fmt, install, notify, sys};

const TICK: Duration = Duration::from_secs(30);
const LEAK_SAMPLES: usize = 20; // 20 × 30 s = 10 min
const LEAK_BYTES: u64 = 1 << 30;

fn pid_file() -> PathBuf {
    config::data_dir().join("vigia.pid")
}

fn alive(pid: i32) -> bool {
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

fn read_pid() -> Option<i32> {
    std::fs::read_to_string(pid_file()).ok()?.trim().parse().ok().filter(|&p| alive(p))
}

pub fn is_running() -> bool {
    read_pid().is_some()
}

pub fn start() -> bool {
    std::process::Command::new(install::installed_exe())
        .arg("--vigia")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// Detiene el vigía y espera a que termine. Devuelve si estaba activo.
pub fn stop_and_wait() -> bool {
    let Some(pid) = read_pid() else { return false };
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }
    for _ in 0..40 {
        if !alive(pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_file(pid_file());
    true
}

fn top_consumers(groups: &[AppGroup], n: usize) -> String {
    groups
        .iter()
        .filter(|g| !g.system)
        .take(n)
        .map(|g| format!("{} {}", g.name, fmt::bytes(g.private)))
        .collect::<Vec<_>>()
        .join(" · ")
}

struct State {
    sampler: Sampler,
    cfg: Config,
    cfg_mtime: Option<SystemTime>,
    last_alert: HashMap<String, Instant>,
    high: u32,
    crit: u32,
    cpu_hi: u32,
    hist: HashMap<String, VecDeque<u64>>,
    startup: Option<HashSet<String>>,
}

impl State {
    fn alert(&mut self, key: &str, cooldown: Duration, title: &str, text: &str) {
        if self.last_alert.get(key).is_some_and(|t| t.elapsed() < cooldown) {
            return;
        }
        self.last_alert.insert(key.to_string(), Instant::now());
        notify::toast(title, text, "");
        config::push_alert(key.split(':').next().unwrap_or(key), &format!("{}: {}", title, text));
    }

    fn tick(&mut self) {
        let mtime = config::config_mtime();
        if mtime != self.cfg_mtime {
            self.cfg = Config::load();
            self.cfg_mtime = mtime;
        }
        let mem = sys::memory();
        let (groups, cpu) = self.sampler.sample(false);

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
                self.alert(
                    &format!("fuga:{}", name),
                    Duration::from_secs(3600),
                    "Posible fuga de memoria",
                    &format!("{} creció {} en 10 min sin parar. Si va lento, guarda tu trabajo y reinícialo.", name, fmt::bytes(grow)),
                );
            }
        }

        // RAM: dos lecturas seguidas (1 min) por encima del umbral para no avisar por picos.
        // Aquí no se estabiliza solo: liberar la caché del sistema pide la contraseña.
        let load = mem.load.min(100) as u8;
        self.crit = if load >= self.cfg.ram_critical { self.crit + 1 } else { 0 };
        self.high = if load >= self.cfg.ram_warn { self.high + 1 } else { 0 };
        if self.crit >= 2 {
            self.alert(
                "critica",
                Duration::from_secs(900),
                "Memoria crítica",
                &format!("RAM al {} %. Más consumo: {}. Abre NEXO › Memoria RAM para estabilizar o cerrar algo.", load, top_consumers(&groups, 3)),
            );
        } else if self.high >= 2 {
            self.alert("ram", Duration::from_secs(1200), "Memoria alta", &format!("RAM al {} %. Más consumo: {}.", load, top_consumers(&groups, 3)));
        }

        if mem.commit_pct() >= 90.0 {
            self.alert(
                "virtual",
                Duration::from_secs(900),
                "Memoria de intercambio casi agotada",
                &format!("Al {:.0} %. Guarda tu trabajo: los programas podrían fallar. Más consumo: {}", mem.commit_pct(), top_consumers(&groups, 2)),
            );
        }

        self.cpu_hi = if cpu >= 90.0 { self.cpu_hi + 1 } else { 0 };
        if self.cfg.alert_cpu && self.cpu_hi >= 6 {
            if let Some(g) = groups.iter().max_by(|a, b| a.cpu_pct.total_cmp(&b.cpu_pct)) {
                let text = format!("Lleva 3 minutos por encima del 90 %. Principal responsable: {} ({:.0} %).", g.name, g.cpu_pct);
                self.alert("cpu", Duration::from_secs(1800), "Procesador al máximo", &text);
            }
        }

        if self.cfg.alert_disk {
            let (free, total) = sys::disk_space(&sys::system_drive());
            if total > 0 && (free as f64 / total as f64) < 0.10 {
                self.alert(
                    "disco",
                    Duration::from_secs(12 * 3600),
                    "Disco casi lleno",
                    &format!("Quedan {} libres. Abre NEXO › Limpieza para recuperar espacio.", fmt::bytes(free)),
                );
            }
        }

        if self.cfg.alert_security {
            let now_entries = sys::startup_entries();
            if let Some(prev) = &self.startup {
                let new: Vec<String> = now_entries.difference(prev).cloned().collect();
                if let Some(first) = new.first() {
                    let more = if new.len() > 1 { format!(" (y {} más)", new.len() - 1) } else { String::new() };
                    self.alert(
                        &format!("inicio:{}", first),
                        Duration::from_secs(24 * 3600),
                        "Nuevo programa al inicio de sesión",
                        &format!("{}{}. Si no lo instalaste tú, revísalo en NEXO › Protección.", first, more),
                    );
                }
            }
            self.startup = Some(now_entries);
        }

        sys::trim_own_memory();
    }
}

pub fn run() {
    if is_running() {
        return;
    }
    let _ = std::fs::create_dir_all(config::data_dir());
    let _ = std::fs::write(pid_file(), std::process::id().to_string());
    sys::background_process();
    let mut st = State {
        sampler: Sampler::default(),
        cfg: Config::load(),
        cfg_mtime: config::config_mtime(),
        last_alert: HashMap::new(),
        high: 0,
        crit: 0,
        cpu_hi: 0,
        hist: HashMap::new(),
        startup: None,
    };
    loop {
        st.tick();
        std::thread::sleep(TICK);
    }
}
