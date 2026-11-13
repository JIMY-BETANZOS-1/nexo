//! Modo automático (lo lanza el Programador de tareas con `--auto`).
//!
//! Solo limpia basura real (categorías seguras). Capturas, descargas, videos y
//! documentos NUNCA se tocan aquí: solo se cuentan para avisarte.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config::{push_history, Config, HistoryEntry};
use crate::rules::{categories, Env, Kind};
use crate::scan::{scan_all, Ctx, Progress};
use crate::{clean, fmt, sys};

/// Instalaciones o actualizaciones en curso (Windows, Linux y macOS).
const BUSY_PROCS: &[&str] = &[
    "msiexec.exe", "tiworker.exe", "setup.exe", "installer.exe", "apt", "apt-get", "dpkg", "dnf", "rpm", "pacman",
    "packagekitd", "installer", "softwareupdated",
];
const RETRIES: u32 = 12;
const RETRY_WAIT: Duration = Duration::from_secs(10 * 60);

/// Devuelve el motivo por el que conviene esperar, o None si se puede limpiar ya.
pub fn wait_reason() -> Option<String> {
    let pw = sys::power();
    if pw.has_battery && !pw.on_ac && pw.percent.is_some_and(|p| p < 40) {
        return Some("batería por debajo del 40 %".into());
    }
    if sys::user_busy() {
        return Some("estás en pantalla completa (juego, video o presentación)".into());
    }
    let procs = sys::running_processes();
    if let Some(p) = BUSY_PROCS.iter().find(|p| procs.contains(**p)) {
        return Some(format!("hay una instalación o actualización en curso ({})", p));
    }
    None
}

/// Escaneo completo de solo lectura volcado a un archivo de texto.
pub fn report(out: &std::path::Path) {
    let env = Env::detect();
    let defs = categories(&env);
    let admin = sys::is_admin();
    let procs = sys::running_processes();
    let progress = Arc::new(Mutex::new(Progress::default()));
    let cancel = AtomicBool::new(false);
    let ctx = Ctx { admin, procs: &procs, progress: &progress, cancel: &cancel };
    let start = std::time::Instant::now();
    let results = scan_all(&defs, None, &ctx);
    let mut s = format!("NEXO · informe de escaneo (solo lectura)\nAdministrador: {}\nTiempo: {:.1} s\n\n", admin, start.elapsed().as_secs_f32());
    for r in &results {
        s += &format!("[{:?}] {:<36} {:>12}  {:>7} elementos\n", r.kind, r.name, fmt::bytes(r.size), r.count);
        for n in &r.notes {
            s += &format!("      nota: {}\n", n);
        }
        for it in r.items.iter().take(5) {
            s += &format!("      {}  ({})\n", it.path.display(), fmt::bytes(it.size));
        }
    }
    let _ = std::fs::write(out, s);
}

/// Recorre todo el camino del modo automático sin borrar nada (para comprobarlo).
pub fn simulate() -> String {
    let env = Env::detect();
    let defs = categories(&env);
    let cfg = Config::load();
    let procs = sys::running_processes();
    let progress = Arc::new(Mutex::new(Progress::default()));
    let cancel = AtomicBool::new(false);
    let ctx = Ctx { admin: sys::is_admin(), procs: &procs, progress: &progress, cancel: &cancel };
    let safe_ids: Vec<String> = cfg
        .auto_categories
        .iter()
        .filter(|id| defs.iter().any(|d| d.id == id.as_str() && d.kind == Kind::Safe && d.special == crate::rules::Special::None))
        .cloned()
        .collect();
    let results = scan_all(&defs, Some(&safe_ids), &ctx);
    let mut s = format!(
        "Condiciones: {}\n",
        wait_reason().unwrap_or_else(|| "se cumplen (limpiaría ahora)".into())
    );
    for r in &results {
        s += &format!("  {:<34} {:>10} {:>7}\n", r.name, fmt::bytes(r.size), r.count);
        if r.kind != Kind::Safe {
            s += "  ERROR: categoría no segura en automático\n";
        }
    }
    let total: u64 = results.iter().map(|r| r.size).sum();
    let prot: usize = results.iter().map(|r| r.items.iter().filter(|i| crate::rules::protection(&i.path) != crate::rules::Protect::No).count()).sum();
    s += &format!("Total que liberaría: {} · videos/documentos incluidos: {}\n", fmt::bytes(total), prot);
    s
}

pub fn run() {
    sys::background_process();
    let mut cfg = Config::load();
    if !cfg.auto_enabled {
        return;
    }
    if let Some(last) = cfg.last_auto_run {
        if fmt::now().saturating_sub(last) < 6 * 3600 {
            return;
        }
    }

    let mut attempt = 0;
    while let Some(reason) = wait_reason() {
        attempt += 1;
        if attempt >= RETRIES {
            push_history(HistoryEntry {
                ts: fmt::now(),
                mode: "pospuesta".into(),
                note: format!("Limpieza pospuesta: {}.", reason),
                ..Default::default()
            });
            return;
        }
        std::thread::sleep(RETRY_WAIT);
    }

    let env = Env::detect();
    let defs = categories(&env);
    let admin = sys::is_admin();
    let procs = sys::running_processes();
    let progress = Arc::new(Mutex::new(Progress::default()));
    let cancel = AtomicBool::new(false);
    let ctx = Ctx { admin, procs: &procs, progress: &progress, cancel: &cancel };

    // Solo categorías seguras elegidas por el usuario.
    let safe_ids: Vec<String> = cfg
        .auto_categories
        .iter()
        .filter(|id| defs.iter().any(|d| d.id == id.as_str() && d.kind == Kind::Safe && d.special == crate::rules::Special::None))
        .cloned()
        .collect();
    let mut results = scan_all(&defs, Some(&safe_ids), &ctx);
    for r in results.iter_mut() {
        r.selected = r.kind == Kind::Safe;
    }
    let report = clean::clean(&defs, &results, &env, &ctx);

    // Solo lectura: cuánto hay para que el usuario revise (no se toca).
    let review_ids: Vec<String> = ["screenshots", "downloads_old", "protected"].iter().map(|s| s.to_string()).collect();
    let review = scan_all(&defs, Some(&review_ids), &ctx);
    let review_bytes: u64 = review.iter().map(|r| r.size).sum();

    push_history(HistoryEntry {
        ts: fmt::now(),
        mode: "auto".into(),
        freed: report.freed,
        files: report.files,
        skipped: report.skipped,
        recycled: 0,
        details: report.per_cat.clone(),
        note: String::new(),
        review_bytes,
    });
    cfg.last_auto_run = Some(fmt::now());
    cfg.save();

    let line2 = if review_bytes > 0 {
        format!("Para revisar (no se tocó): {} en capturas, descargas, videos y documentos. Abre NEXO para decidir.", fmt::bytes(review_bytes))
    } else {
        "Tus videos y documentos no se tocaron.".to_string()
    };
    crate::notify::toast(
        "Limpieza automática",
        &format!("Liberados {} ({} archivos de basura).", fmt::bytes(report.freed), fmt::count(report.files)),
        &line2,
    );
}
