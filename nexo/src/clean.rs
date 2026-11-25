// Proyecto: NEXO
// Archivo: clean.rs
// Autor: Chatta Betanzos, Jimy Alexander (jimy-betanzos)

//! Limpieza. Solo actúa sobre lo que el usuario dejó seleccionado.
//!
//! - Categorías seguras (basura del sistema): borrado definitivo.
//! - Revisión y protegidos (capturas, descargas, videos, documentos): a la Papelera.

// jimy-betanzos - dependencia: biblioteca estándar de Rust (fs)
use std::fs;
// jimy-betanzos - dependencia: biblioteca estándar de Rust (path)
use std::path::{Path, PathBuf};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (sync)
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (sync)
use std::sync::Mutex;

// jimy-betanzos - dependencia: módulo propio rules
use crate::rules::{jb_is_inside, jb_protection, JbCategoryDef, JbEnv, JbKind, JbProtect, JbSpecial, JbTarget};
// jimy-betanzos - dependencia: módulo propio scan
use crate::scan::{jb_blocked_by, jb_walk, JbCatResult, JbCtx};
// jimy-betanzos - dependencia: módulo propio sys
use crate::sys;

/// jimy-betanzos: Resultado de una limpieza: archivos, espacio liberado y errores.
#[derive(Default, Clone)]
pub struct JbCleanReport {
    pub freed: u64,
    pub files: u64,
    pub skipped: u64,
    pub recycled: u64,
    pub per_cat: Vec<(String, u64)>,
}

/// jimy-betanzos: Borra un archivo aunque sea de solo lectura.
fn jb_remove_file_force(p: &Path) -> std::io::Result<()> {
    match fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let m = fs::metadata(p)?;
            let mut perms = m.permissions();
            if perms.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                fs::set_permissions(p, perms)?;
                fs::remove_file(p)
            } else {
                Err(e)
            }
        }
        Err(e) => Err(e),
    }
}

/// jimy-betanzos: Borra la basura de un destino seguro. Devuelve (liberado, archivos, omitidos).
fn jb_clean_target(t: &JbTarget, env: &JbEnv, ctx: &JbCtx, files_total: &AtomicU64, bytes_total: &AtomicU64) -> (u64, u64, u64) {
    let (mut freed, mut files, mut skipped) = (0u64, 0u64, 0u64);
    let mut dirs: Vec<PathBuf> = Vec::new();
    let min_age = t.min_age_h * 3600;
    jb_walk(
        t,
        ctx.cancel,
        |path, size| {
            // Doble seguro: jamás borrar videos/documentos ni nada de carpetas personales.
            if jb_protection(path) != JbProtect::No || env.jb_is_forbidden(path) || !jb_is_inside(path, &t.root) {
                skipped += 1;
                return;
            }
            match jb_remove_file_force(path) {
                Ok(()) => {
                    freed += size;
                    files += 1;
                    let n = files_total.fetch_add(1, Ordering::Relaxed) + 1;
                    let b = bytes_total.fetch_add(size, Ordering::Relaxed) + size;
                    if n.is_multiple_of(256) {
                        if let Ok(mut p) = ctx.progress.try_lock() {
                            p.files = n;
                            p.bytes = b;
                            p.current = path.display().to_string();
                        }
                    }
                }
                Err(_) => skipped += 1, // en uso o sin permiso: se deja
            }
        },
        |dir, age| {
            if t.min_age_h > 0 && age >= min_age {
                dirs.push(dir.to_path_buf());
            }
        },
    );
    // Carpetas vacías y viejas dentro de temporales (de la más profunda a la raíz).
    for dir in dirs.iter().rev() {
        let _ = fs::remove_dir(dir);
    }
    (freed, files, skipped)
}

// jimy-betanzos: lo «Para revisar» y lo «Protegido» va a la Papelera (se puede recuperar);
// solo la basura del sistema se elimina de forma definitiva.
/// jimy-betanzos: Limpia las categorías seleccionadas y devuelve el resumen.
pub fn jb_clean(defs: &[JbCategoryDef], results: &[JbCatResult], env: &JbEnv, ctx: &JbCtx) -> JbCleanReport {
    let mut rep = JbCleanReport::default();
    let selected: Vec<(&JbCatResult, &JbCategoryDef)> = results
        .iter()
        .filter(|r| r.jb_selected_size() > 0 || (r.jb_whole() && r.selected && r.count > 0))
        .filter_map(|r| defs.iter().find(|d| d.id == r.id).map(|d| (r, d)))
        .collect();
    let mut freed_by_cat = vec![0u64; selected.len()];

    // 1) Basura segura: todos sus destinos se borran a la vez.
    let jobs: Vec<(usize, JbTarget)> = selected
        .iter()
        .enumerate()
        .filter(|(_, (_, d))| d.kind == JbKind::Safe && d.special == JbSpecial::None)
        .flat_map(|(i, (_, d))| d.targets.iter().map(move |t| (i, t)))
        .filter(|(_, t)| !(t.admin && !ctx.admin) && jb_blocked_by(t, ctx.procs).is_none())
        .flat_map(|(i, t)| crate::scan::jb_split_target(t).into_iter().map(move |part| (i, part)))
        .collect();
    if let Ok(mut p) = ctx.progress.lock() {
        p.steps = jobs.len() + selected.len();
        p.label = "Borrando basura en paralelo".into();
    }
    let next = AtomicUsize::new(0);
    let files_total = AtomicU64::new(0);
    let bytes_total = AtomicU64::new(0);
    let partials: Vec<Mutex<(u64, u64, u64)>> = jobs.iter().map(|_| Mutex::new((0, 0, 0))).collect();
    std::thread::scope(|s| {
        for _ in 0..crate::scan::jb_workers().min(jobs.len().max(1)) {
            s.spawn(|| {
                sys::jb_interactive_thread();
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= jobs.len() || ctx.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let r = jb_clean_target(&jobs[k].1, env, ctx, &files_total, &bytes_total);
                    if let Ok(mut slot) = partials[k].lock() {
                        *slot = r;
                    }
                    if let Ok(mut p) = ctx.progress.lock() {
                        p.step += 1;
                    }
                }
            });
        }
    });
    for (k, (i, _)) in jobs.iter().enumerate() {
        let (f, n, sk) = *partials[k].lock().unwrap_or_else(|e| e.into_inner());
        freed_by_cat[*i] += f;
        rep.files += n;
        rep.skipped += sk;
    }

    // 2) Papelera de reciclaje, capturas, descargas, videos y documentos (en orden).
    for (i, (r, d)) in selected.iter().enumerate() {
        if d.kind == JbKind::Safe && d.special == JbSpecial::None {
            continue;
        }
        if let Ok(mut p) = ctx.progress.lock() {
            p.label = d.name.to_string();
            p.step += 1;
        }
        let mut freed = 0u64;
        if let JbSpecial::System(id) = d.special {
            if r.selected {
                if let Ok(f) = crate::rules::jb_system_clean(id) {
                    freed = f;
                    rep.files += r.count;
                }
            }
        } else if d.special == JbSpecial::RecycleBin {
            if r.selected && sys::jb_empty_recycle_bin() {
                freed = r.size;
                rep.files += r.count;
            }
        } else {
            // Revisión / protegidos: solo lo marcado, siempre a la Papelera.
            let picked: Vec<(&PathBuf, u64)> = r
                .items
                .iter()
                .filter(|it| it.selected && it.path.exists())
                .map(|it| (&it.path, it.size))
                .collect();
            // En lotes: cada envío a la Papelera prepara la operación de Windows desde cero,
            // así que mandar 200 de una vez es muchísimo más rápido que uno por uno.
            for chunk in picked.chunks(200) {
                if ctx.cancel.load(Ordering::Relaxed) {
                    break;
                }
                let batch_ok = trash::delete_all(chunk.iter().map(|(p, _)| *p)).is_ok();
                for (path, size) in chunk {
                    // Si el lote falló (p. ej. un archivo en uso), se reintenta solo lo que quedó.
                    let gone = if batch_ok { true } else { !path.exists() || trash::delete(path).is_ok() };
                    if gone {
                        freed += size;
                        rep.files += 1;
                        rep.recycled += 1;
                    } else {
                        rep.skipped += 1;
                    }
                }
                if let Ok(mut p) = ctx.progress.lock() {
                    p.files = rep.files;
                    p.bytes = bytes_total.load(Ordering::Relaxed) + freed;
                    if let Some((last, _)) = chunk.last() {
                        p.current = last.display().to_string();
                    }
                }
            }
        }
        freed_by_cat[i] += freed;
    }

    for (i, (_, d)) in selected.iter().enumerate() {
        rep.freed += freed_by_cat[i];
        rep.per_cat.push((d.name.to_string(), freed_by_cat[i]));
    }
    rep
}
