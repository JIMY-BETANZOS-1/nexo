// ════════════════════════════════════════════════════════════════════════
//  NEXO · nexo/src/scan.rs
//  Autor: Chatta Betanzos, Jimy Alexander  ·  jimy-betanzos
// ════════════════════════════════════════════════════════════════════════

//! Motor de escaneo. Solo lee: nunca borra nada.

// jimy-betanzos · dependencia: biblioteca estándar de Rust (collections)
use std::collections::HashSet;
// jimy-betanzos · dependencia: biblioteca estándar de Rust (path)
use std::path::{Path, PathBuf};
// jimy-betanzos · dependencia: biblioteca estándar de Rust (sync)
use std::sync::atomic::{AtomicBool, Ordering};
// jimy-betanzos · dependencia: biblioteca estándar de Rust (sync)
use std::sync::{Arc, Mutex};
// jimy-betanzos · dependencia: biblioteca estándar de Rust (time)
use std::time::SystemTime;

// jimy-betanzos · dependencia: recorrer carpetas (walkdir)
use walkdir::WalkDir;

// jimy-betanzos · dependencia: módulo propio rules
use crate::rules::{jb_protection, JbCategoryDef, JbFilter, JbKind, JbProtect, JbSpecial, JbTarget};
// jimy-betanzos · dependencia: módulo propio sys
use crate::sys;

// jimy-betanzos: no seguir enlaces evita salir de la carpeta analizada y borrar algo ajeno.
/// Enlace o unión (reparse point en Windows, enlace simbólico en Linux y macOS): nunca se sigue.
#[cfg(windows)]
fn jb_is_link(m: &std::fs::Metadata) -> bool {
    // jimy-betanzos · dependencia: biblioteca estándar de Rust (os)
    use std::os::windows::fs::MetadataExt;
    m.file_attributes() & 0x400 != 0
}
/// jimy-betanzos · Indica si una entrada es un enlace o unión.
#[cfg(unix)]
fn jb_is_link(m: &std::fs::Metadata) -> bool {
    m.file_type().is_symlink()
}
/// En categorías seguras solo guardamos una muestra (ahorra RAM con miles de temporales).
const SAMPLE: usize = 40;
const REVIEW_LIMIT: usize = 5000;

/// jimy-betanzos · Avance del escaneo: archivos vistos y carpeta actual.
#[derive(Default, Clone)]
pub struct JbProgress {
    pub label: String,
    pub current: String,
    pub files: u64,
    pub bytes: u64,
    pub step: usize,
    pub steps: usize,
    /// Al limpiar: bytes seleccionados, para un porcentaje real.
    pub total_bytes: u64,
}

/// jimy-betanzos · Avance compartido entre los hilos del escaneo.
pub type JbShared = Arc<Mutex<JbProgress>>;

/// jimy-betanzos · Archivo encontrado con su tamaño y fecha.
#[derive(Clone)]
pub struct JbFoundItem {
    pub path: PathBuf,
    pub size: u64,
    pub selected: bool,
    pub origin: &'static str,
}

/// jimy-betanzos · Resultado del escaneo de una categoría.
#[derive(Clone)]
pub struct JbCatResult {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub kind: JbKind,
    pub special: JbSpecial,
    pub admin_only: bool,
    pub size: u64,
    pub count: u64,
    pub items: Vec<JbFoundItem>,
    pub selected: bool,
    pub notes: Vec<String>,
    pub expanded: bool,
    /// Archivo mostrado en el visor (el último que se señaló).
    pub preview: Option<usize>,
}

/// jimy-betanzos · Métodos de CatResult.
impl JbCatResult {
    /// jimy-betanzos · Categorías que se marcan como bloque (no archivo por archivo).
    pub fn jb_whole(&self) -> bool {
        self.kind == JbKind::Safe || self.special != JbSpecial::None
    }

    /// jimy-betanzos · Tamaño de lo que está seleccionado para limpiar.
    pub fn jb_selected_size(&self) -> u64 {
        if self.jb_whole() {
            if self.selected { self.size } else { 0 }
        } else {
            self.items.iter().filter(|i| i.selected).map(|i| i.size).sum()
        }
    }

    /// jimy-betanzos · Cantidad de archivos seleccionados para limpiar.
    pub fn jb_selected_count(&self) -> u64 {
        if self.jb_whole() {
            if self.selected { self.count } else { 0 }
        } else {
            self.items.iter().filter(|i| i.selected).count() as u64
        }
    }
}

/// jimy-betanzos · Contexto del escaneo: avance, cancelación y procesos abiertos.
pub struct JbCtx<'a> {
    pub admin: bool,
    pub procs: &'a HashSet<String>,
    pub progress: &'a JbShared,
    pub cancel: &'a AtomicBool,
}

/// jimy-betanzos · Programa abierto que impide tocar un destino.
pub fn jb_blocked_by(t: &JbTarget, procs: &HashSet<String>) -> Option<&'static str> {
    t.blocked_by.iter().find(|p| procs.contains(**p)).copied()
}

/// jimy-betanzos · Indica si un archivo cumple el filtro del destino.
fn jb_matches(filter: &JbFilter, p: &Path) -> bool {
    match filter {
        JbFilter::All => true,
        JbFilter::Prefix(pre) => p
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase().starts_with(pre))
            .unwrap_or(false),
        JbFilter::Ext(list) => list.contains(&crate::rules::jb_ext_of(p).as_str()),
        JbFilter::ProtectedOnly => jb_protection(p) != JbProtect::No,
    }
}

/// jimy-betanzos · Antigüedad de un archivo en segundos.
fn jb_age_secs(m: &std::fs::Metadata, now: SystemTime) -> Option<u64> {
    let mt = m.modified().ok()?;
    Some(now.duration_since(mt).map(|d| d.as_secs()).unwrap_or(0))
}

/// jimy-betanzos · Recorre un destino aplicando filtro y antigüedad. Nunca sigue enlaces ni uniones.
pub fn jb_walk(
    t: &JbTarget,
    cancel: &AtomicBool,
    mut on_file: impl FnMut(&Path, u64),
    mut on_dir: impl FnMut(&Path, u64),
) {
    let now = SystemTime::now();
    let depth = if t.recursive { usize::MAX } else { 1 };
    let it = WalkDir::new(&t.root).follow_links(false).max_depth(depth).into_iter().filter_entry(|e| {
        e.depth() == 0
            || e.metadata().map(|m| !jb_is_link(&m)).unwrap_or(false)
    });
    for e in it {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let Ok(e) = e else { continue };
        if e.depth() == 0 {
            continue;
        }
        let Ok(m) = e.metadata() else { continue };
        if m.is_dir() {
            on_dir(e.path(), jb_age_secs(&m, now).unwrap_or(0));
            continue;
        }
        if !m.is_file() || jb_is_link(&m) {
            continue;
        }
        if !jb_matches(&t.filter, e.path()) {
            continue;
        }
        if t.min_age_h > 0 {
            match jb_age_secs(&m, now) {
                Some(a) if a >= t.min_age_h * 3600 => {}
                _ => continue,
            }
        }
        on_file(e.path(), m.len());
    }
}

/// jimy-betanzos · Hilos para recorrer carpetas a la vez (el SSD aguanta muchas lecturas en paralelo).
pub fn jb_workers() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8)
}

/// jimy-betanzos · Divide un destino recursivo en "archivos sueltos de la raíz" + una tarea por subcarpeta,
/// para que varios hilos lo recorran a la vez. Las reglas (filtro, antigüedad, programas
/// que lo bloquean) son las mismas: solo cambia quién recorre cada parte.
pub fn jb_split_target(t: &JbTarget) -> Vec<JbTarget> {
    if !t.recursive {
        return vec![t.clone()];
    }
    let mut v = vec![JbTarget { recursive: false, ..t.clone() }];
    if let Ok(rd) = std::fs::read_dir(&t.root) {
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() && !jb_is_link(&m) {
                v.push(JbTarget { root: e.path(), ..t.clone() });
            }
        }
    }
    v
}

/// jimy-betanzos · Lo que encuentra un hilo en un destino; luego se une con el resto de su categoría.
#[derive(Default)]
struct JbPartial {
    size: u64,
    count: u64,
    items: Vec<JbFoundItem>,
    set_aside: Vec<JbFoundItem>,
}

/// jimy-betanzos · Escanea todas las categorías en paralelo (solo lectura).
pub fn jb_scan_all(defs: &[JbCategoryDef], only: Option<&[String]>, ctx: &JbCtx) -> Vec<JbCatResult> {
    // jimy-betanzos · dependencia: biblioteca estándar de Rust (sync)
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    let chosen: Vec<&JbCategoryDef> = defs
        .iter()
        .filter(|d| only.is_none_or(|o| o.iter().any(|x| x == d.id)))
        .collect();

    // 1) Preparar cada categoría y la lista de destinos a recorrer.
    let mut out: Vec<JbCatResult> = Vec::new();
    let mut jobs: Vec<(usize, &JbCategoryDef, JbTarget)> = Vec::new();
    for d in &chosen {
        let mut r = JbCatResult {
            id: d.id,
            name: d.name,
            desc: d.desc,
            kind: d.kind,
            special: d.special,
            admin_only: d.jb_admin_only(),
            size: 0,
            count: 0,
            items: Vec::new(),
            selected: d.default_on,
            notes: Vec::new(),
            expanded: false,
            preview: None,
        };
        if d.special == JbSpecial::RecycleBin {
            let (s, n) = sys::jb_recycle_bin();
            r.size = s;
            r.count = n;
        }
        if let JbSpecial::System(id) = d.special {
            let (s, n) = crate::rules::jb_system_size(id);
            r.size = s;
            r.count = n;
            r.notes.push("Lo limpia un comando del sistema: te pedirá tu contraseña.".into());
        }
        let mut skipped_admin = false;
        for t in &d.targets {
            if t.admin && !ctx.admin {
                skipped_admin = true;
                continue;
            }
            if let Some(app) = jb_blocked_by(t, ctx.procs) {
                let note = format!("Omitido en parte: {} está abierto (ciérralo para limpiarlo).", app);
                if !r.notes.contains(&note) {
                    r.notes.push(note);
                }
                continue;
            }
            for part in jb_split_target(t) {
                jobs.push((out.len(), *d, part));
            }
        }
        if skipped_admin {
            r.notes.push(if r.admin_only {
                "Requiere permisos de administrador.".into()
            } else {
                "Una parte requiere permisos de administrador.".into()
            });
        }
        if r.admin_only && !ctx.admin {
            r.selected = false;
        }
        out.push(r);
    }
    if let Ok(mut p) = ctx.progress.lock() {
        p.steps = jobs.len().max(1);
        p.label = "Analizando en paralelo".into();
    }

    // 2) Recorrer todos los destinos a la vez.
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let files_seen = AtomicU64::new(0);
    let partials: Vec<Mutex<JbPartial>> = jobs.iter().map(|_| Mutex::new(JbPartial::default())).collect();
    std::thread::scope(|s| {
        for _ in 0..jb_workers().min(jobs.len().max(1)) {
            s.spawn(|| {
                sys::jb_interactive_thread();
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= jobs.len() || ctx.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let (_, d, t) = (&jobs[k].0, jobs[k].1, &jobs[k].2);
                    let mut part = JbPartial::default();
                    jb_walk(
                        t,
                        ctx.cancel,
                        |path, size| {
                            let n = files_seen.fetch_add(1, Ordering::Relaxed) + 1;
                            if n.is_multiple_of(512) {
                                if let Ok(mut p) = ctx.progress.try_lock() {
                                    p.files = n;
                                    p.current = path.display().to_string();
                                }
                            }
                            if d.kind == JbKind::Safe && jb_protection(path) != JbProtect::No {
                                // Un video o documento dentro de una carpeta de basura: se aparta.
                                part.set_aside.push(JbFoundItem { path: path.to_path_buf(), size, selected: false, origin: d.name });
                                return;
                            }
                            part.size += size;
                            part.count += 1;
                            let limit = if d.kind == JbKind::Safe { SAMPLE } else { REVIEW_LIMIT };
                            if part.items.len() < limit {
                                let selected = d.kind == JbKind::Safe || d.default_on;
                                part.items.push(JbFoundItem { path: path.to_path_buf(), size, selected, origin: d.name });
                            }
                        },
                        |_, _| {},
                    );
                    if let Ok(mut slot) = partials[k].lock() {
                        *slot = part;
                    }
                    let fin = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if let Ok(mut p) = ctx.progress.lock() {
                        p.step = fin.saturating_sub(1);
                        p.label = d.name.to_string();
                    }
                }
            });
        }
    });

    // 3) Unir los resultados por categoría (en el mismo orden de siempre).
    let mut set_aside: Vec<JbFoundItem> = Vec::new();
    for (k, (ci, d, _)) in jobs.iter().enumerate() {
        let part = std::mem::take(&mut *partials[k].lock().unwrap_or_else(|e| e.into_inner()));
        let r = &mut out[*ci];
        r.size += part.size;
        r.count += part.count;
        let limit = if d.kind == JbKind::Safe { SAMPLE } else { REVIEW_LIMIT };
        for it in part.items {
            if r.items.len() < limit {
                r.items.push(it);
            }
        }
        set_aside.extend(part.set_aside);
    }

    if let Some(p) = out.iter_mut().find(|r| r.kind == JbKind::Protected) {
        for it in set_aside {
            p.size += it.size;
            p.count += 1;
            p.items.push(it);
        }
    }
    for r in out.iter_mut().filter(|r| !r.jb_whole()) {
        r.items.sort_by_key(|i| std::cmp::Reverse(i.size));
        // El tamaño mostrado es el de los elementos listados, para que cuadre con la selección.
        r.size = r.items.iter().map(|i| i.size).sum();
        r.count = r.items.len() as u64;
    }
    if let Ok(mut p) = ctx.progress.lock() {
        p.files = files_seen.load(Ordering::Relaxed);
    }
    out
}
