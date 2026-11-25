use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config::{jb_push_history, JbConfig, JbHistoryEntry};
use crate::rules::{jb_categories, JbEnv, JbKind};
use crate::scan::{jb_scan_all, JbCtx, JbProgress};
use crate::{clean, fmt, sys};

const BUSY_PROCS: &[&str] = &[
    "msiexec.exe", "tiworker.exe", "setup.exe", "installer.exe", "apt", "apt-get", "dpkg", "dnf", "rpm", "pacman",
    "packagekitd", "installer", "softwareupdated",
];
const RETRIES: u32 = 12;
const RETRY_WAIT: Duration = Duration::from_secs(10 * 60);

pub fn jb_wait_reason() -> Option<String> {
    let pw = sys::jb_power();
    if pw.has_battery && !pw.on_ac && pw.percent.is_some_and(|p| p < 40) {
        return Some("batería por debajo del 40 %".into());
    }
    if sys::jb_user_busy() {
        return Some("estás en pantalla completa (juego, video o presentación)".into());
    }
    let procs = sys::jb_running_processes();
    if let Some(p) = BUSY_PROCS.iter().find(|p| procs.contains(**p)) {
        return Some(format!("hay una instalación o actualización en curso ({})", p));
    }
    None
}

pub fn jb_report(out: &std::path::Path) {
    let env = JbEnv::jb_detect();
    let defs = jb_categories(&env);
    let admin = sys::jb_is_admin();
    let procs = sys::jb_running_processes();
    let progress = Arc::new(Mutex::new(JbProgress::default()));
    let cancel = AtomicBool::new(false);
    let ctx = JbCtx { admin, procs: &procs, progress: &progress, cancel: &cancel };
    let start = std::time::Instant::now();
    let results = jb_scan_all(&defs, None, &ctx);
    let mut s = format!("NEXO · informe de escaneo (solo lectura)\nAdministrador: {}\nTiempo: {:.1} s\n\n", admin, start.elapsed().as_secs_f32());
    for r in &results {
        s += &format!("[{:?}] {:<36} {:>12}  {:>7} elementos\n", r.kind, r.name, fmt::jb_bytes(r.size), r.count);
        for n in &r.notes {
            s += &format!("      nota: {}\n", n);
        }
        for it in r.items.iter().take(5) {
            s += &format!("      {}  ({})\n", it.path.display(), fmt::jb_bytes(it.size));
        }
    }
    let _ = std::fs::write(out, s);
}

pub fn jb_simulate() -> String {
    let env = JbEnv::jb_detect();
    let defs = jb_categories(&env);
    let cfg = JbConfig::jb_load();
    let procs = sys::jb_running_processes();
    let progress = Arc::new(Mutex::new(JbProgress::default()));
    let cancel = AtomicBool::new(false);
    let ctx = JbCtx { admin: sys::jb_is_admin(), procs: &procs, progress: &progress, cancel: &cancel };
    let safe_ids: Vec<String> = cfg
        .auto_categories
        .iter()
        .filter(|id| defs.iter().any(|d| d.id == id.as_str() && d.kind == JbKind::Safe && d.special == crate::rules::JbSpecial::None))
        .cloned()
        .collect();
    let results = jb_scan_all(&defs, Some(&safe_ids), &ctx);
    let mut s = format!(
        "Condiciones: {}\n",
        jb_wait_reason().unwrap_or_else(|| "se cumplen (limpiaría ahora)".into())
    );
    for r in &results {
        s += &format!("  {:<34} {:>10} {:>7}\n", r.name, fmt::jb_bytes(r.size), r.count);
        if r.kind != JbKind::Safe {
            s += "  ERROR: categoría no segura en automático\n";
        }
    }
    let total: u64 = results.iter().map(|r| r.size).sum();
    let prot: usize = results.iter().map(|r| r.items.iter().filter(|i| crate::rules::jb_protection(&i.path) != crate::rules::JbProtect::No).count()).sum();
    s += &format!("Total que liberaría: {} · videos/documentos incluidos: {}\n", fmt::jb_bytes(total), prot);
    s
}

pub fn jb_run() {
    sys::jb_background_process();
    let mut cfg = JbConfig::jb_load();
    if !cfg.auto_enabled {
        return;
    }
    if let Some(last) = cfg.last_auto_run {
        if fmt::jb_now().saturating_sub(last) < 6 * 3600 {
            return;
        }
    }

    let mut attempt = 0;
    while let Some(reason) = jb_wait_reason() {
        attempt += 1;
        if attempt >= RETRIES {
            jb_push_history(JbHistoryEntry {
                ts: fmt::jb_now(),
                mode: "pospuesta".into(),
                note: format!("Limpieza pospuesta: {}.", reason),
                ..Default::default()
            });
            return;
        }
        std::thread::sleep(RETRY_WAIT);
    }

    let env = JbEnv::jb_detect();
    let defs = jb_categories(&env);
    let admin = sys::jb_is_admin();
    let procs = sys::jb_running_processes();
    let progress = Arc::new(Mutex::new(JbProgress::default()));
    let cancel = AtomicBool::new(false);
    let ctx = JbCtx { admin, procs: &procs, progress: &progress, cancel: &cancel };

    let safe_ids: Vec<String> = cfg
        .auto_categories
        .iter()
        .filter(|id| defs.iter().any(|d| d.id == id.as_str() && d.kind == JbKind::Safe && d.special == crate::rules::JbSpecial::None))
        .cloned()
        .collect();
    let mut results = jb_scan_all(&defs, Some(&safe_ids), &ctx);
    for r in results.iter_mut() {
        r.selected = r.kind == JbKind::Safe;
    }
    let report = clean::jb_clean(&defs, &results, &env, &ctx);

    let review_ids: Vec<String> = ["screenshots", "downloads_old", "protected"].iter().map(|s| s.to_string()).collect();
    let review = jb_scan_all(&defs, Some(&review_ids), &ctx);
    let review_bytes: u64 = review.iter().map(|r| r.size).sum();

    jb_push_history(JbHistoryEntry {
        ts: fmt::jb_now(),
        mode: "auto".into(),
        freed: report.freed,
        files: report.files,
        skipped: report.skipped,
        recycled: 0,
        details: report.per_cat.clone(),
        note: String::new(),
        review_bytes,
    });
    cfg.last_auto_run = Some(fmt::jb_now());
    cfg.jb_save();

    let line2 = if review_bytes > 0 {
        format!("Para revisar (no se tocó): {} en capturas, descargas, videos y documentos. Abre NEXO para decidir.", fmt::jb_bytes(review_bytes))
    } else {
        "Tus videos y documentos no se tocaron.".to_string()
    };
    crate::notify::jb_toast(
        "Limpieza automática",
        &format!("Liberados {} ({} archivos de basura).", fmt::jb_bytes(report.freed), fmt::jb_count(report.files)),
        &line2,
    );
}
