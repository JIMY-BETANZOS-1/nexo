//! Motor de escaneo. Solo lee: nunca borra nada.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use walkdir::WalkDir;

use crate::rules::{protection, CategoryDef, Filter, Kind, Protect, Special, Target};
use crate::sys;

/// Enlace o unión (reparse point en Windows, enlace simbólico en Linux y macOS): nunca se sigue.
#[cfg(windows)]
fn is_link(m: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes() & 0x400 != 0
}
#[cfg(unix)]
fn is_link(m: &std::fs::Metadata) -> bool {
    m.file_type().is_symlink()
}
/// En categorías seguras solo guardamos una muestra (ahorra RAM con miles de temporales).
const SAMPLE: usize = 40;
const REVIEW_LIMIT: usize = 5000;

#[derive(Default, Clone)]
pub struct Progress {
    pub label: String,
    pub current: String,
    pub files: u64,
    pub bytes: u64,
    pub step: usize,
    pub steps: usize,
    /// Al limpiar: bytes seleccionados, para un porcentaje real.
    pub total_bytes: u64,
}

pub type Shared = Arc<Mutex<Progress>>;

#[derive(Clone)]
pub struct FoundItem {
    pub path: PathBuf,
    pub size: u64,
    pub selected: bool,
    pub origin: &'static str,
}

#[derive(Clone)]
pub struct CatResult {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub kind: Kind,
    pub special: Special,
    pub admin_only: bool,
    pub size: u64,
    pub count: u64,
    pub items: Vec<FoundItem>,
    pub selected: bool,
    pub notes: Vec<String>,
    pub expanded: bool,
    /// Archivo mostrado en el visor (el último que se señaló).
    pub preview: Option<usize>,
}

impl CatResult {
    /// Categorías que se marcan como bloque (no archivo por archivo).
    pub fn whole(&self) -> bool {
        self.kind == Kind::Safe || self.special != Special::None
    }

    pub fn selected_size(&self) -> u64 {
        if self.whole() {
            if self.selected { self.size } else { 0 }
        } else {
            self.items.iter().filter(|i| i.selected).map(|i| i.size).sum()
        }
    }

    pub fn selected_count(&self) -> u64 {
        if self.whole() {
            if self.selected { self.count } else { 0 }
        } else {
            self.items.iter().filter(|i| i.selected).count() as u64
        }
    }
}

pub struct Ctx<'a> {
    pub admin: bool,
    pub procs: &'a HashSet<String>,
    pub progress: &'a Shared,
    pub cancel: &'a AtomicBool,
}

pub fn blocked_by(t: &Target, procs: &HashSet<String>) -> Option<&'static str> {
    t.blocked_by.iter().find(|p| procs.contains(**p)).copied()
}

fn matches(filter: &Filter, p: &Path) -> bool {
    match filter {
        Filter::All => true,
        Filter::Prefix(pre) => p
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase().starts_with(pre))
            .unwrap_or(false),
        Filter::Ext(list) => list.contains(&crate::rules::ext_of(p).as_str()),
        Filter::ProtectedOnly => protection(p) != Protect::No,
    }
}

fn age_secs(m: &std::fs::Metadata, now: SystemTime) -> Option<u64> {
    let mt = m.modified().ok()?;
    Some(now.duration_since(mt).map(|d| d.as_secs()).unwrap_or(0))
}

/// Recorre un destino aplicando filtro y antigüedad. Nunca sigue enlaces ni uniones.
pub fn walk(
    t: &Target,
    cancel: &AtomicBool,
    mut on_file: impl FnMut(&Path, u64),
    mut on_dir: impl FnMut(&Path, u64),
) {
    let now = SystemTime::now();
    let depth = if t.recursive { usize::MAX } else { 1 };
    let it = WalkDir::new(&t.root).follow_links(false).max_depth(depth).into_iter().filter_entry(|e| {
        e.depth() == 0
            || e.metadata().map(|m| !is_link(&m)).unwrap_or(false)
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
            on_dir(e.path(), age_secs(&m, now).unwrap_or(0));
            continue;
        }
        if !m.is_file() || is_link(&m) {
            continue;
        }
        if !matches(&t.filter, e.path()) {
            continue;
        }
        if t.min_age_h > 0 {
            match age_secs(&m, now) {
                Some(a) if a >= t.min_age_h * 3600 => {}
                _ => continue,
            }
        }
        on_file(e.path(), m.len());
    }
}

/// Hilos para recorrer carpetas a la vez (el SSD aguanta muchas lecturas en paralelo).
pub fn workers() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8)
}

/// Divide un destino recursivo en "archivos sueltos de la raíz" + una tarea por subcarpeta,
/// para que varios hilos lo recorran a la vez. Las reglas (filtro, antigüedad, programas
/// que lo bloquean) son las mismas: solo cambia quién recorre cada parte.
pub fn split_target(t: &Target) -> Vec<Target> {
    if !t.recursive {
        return vec![t.clone()];
    }
    let mut v = vec![Target { recursive: false, ..t.clone() }];
    if let Ok(rd) = std::fs::read_dir(&t.root) {
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() && !is_link(&m) {
                v.push(Target { root: e.path(), ..t.clone() });
            }
        }
    }
    v
}

/// Lo que encuentra un hilo en un destino; luego se une con el resto de su categoría.
#[derive(Default)]
struct Partial {
    size: u64,
    count: u64,
    items: Vec<FoundItem>,
    set_aside: Vec<FoundItem>,
}

pub fn scan_all(defs: &[CategoryDef], only: Option<&[String]>, ctx: &Ctx) -> Vec<CatResult> {
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    let chosen: Vec<&CategoryDef> = defs
        .iter()
        .filter(|d| only.is_none_or(|o| o.iter().any(|x| x == d.id)))
        .collect();

    // 1) Preparar cada categoría y la lista de destinos a recorrer.
    let mut out: Vec<CatResult> = Vec::new();
    let mut jobs: Vec<(usize, &CategoryDef, Target)> = Vec::new();
    for d in &chosen {
        let mut r = CatResult {
            id: d.id,
            name: d.name,
            desc: d.desc,
            kind: d.kind,
            special: d.special,
            admin_only: d.admin_only(),
            size: 0,
            count: 0,
            items: Vec::new(),
            selected: d.default_on,
            notes: Vec::new(),
            expanded: false,
            preview: None,
        };
        if d.special == Special::RecycleBin {
            let (s, n) = sys::recycle_bin();
            r.size = s;
            r.count = n;
        }
        if let Special::System(id) = d.special {
            let (s, n) = crate::rules::system_size(id);
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
            if let Some(app) = blocked_by(t, ctx.procs) {
                let note = format!("Omitido en parte: {} está abierto (ciérralo para limpiarlo).", app);
                if !r.notes.contains(&note) {
                    r.notes.push(note);
                }
                continue;
            }
            for part in split_target(t) {
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
    let partials: Vec<Mutex<Partial>> = jobs.iter().map(|_| Mutex::new(Partial::default())).collect();
    std::thread::scope(|s| {
        for _ in 0..workers().min(jobs.len().max(1)) {
            s.spawn(|| {
                sys::interactive_thread();
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= jobs.len() || ctx.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let (_, d, t) = (&jobs[k].0, jobs[k].1, &jobs[k].2);
                    let mut part = Partial::default();
                    walk(
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
                            if d.kind == Kind::Safe && protection(path) != Protect::No {
                                // Un video o documento dentro de una carpeta de basura: se aparta.
                                part.set_aside.push(FoundItem { path: path.to_path_buf(), size, selected: false, origin: d.name });
                                return;
                            }
                            part.size += size;
                            part.count += 1;
                            let limit = if d.kind == Kind::Safe { SAMPLE } else { REVIEW_LIMIT };
                            if part.items.len() < limit {
                                let selected = d.kind == Kind::Safe || d.default_on;
                                part.items.push(FoundItem { path: path.to_path_buf(), size, selected, origin: d.name });
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
    let mut set_aside: Vec<FoundItem> = Vec::new();
    for (k, (ci, d, _)) in jobs.iter().enumerate() {
        let part = std::mem::take(&mut *partials[k].lock().unwrap_or_else(|e| e.into_inner()));
        let r = &mut out[*ci];
        r.size += part.size;
        r.count += part.count;
        let limit = if d.kind == Kind::Safe { SAMPLE } else { REVIEW_LIMIT };
        for it in part.items {
            if r.items.len() < limit {
                r.items.push(it);
            }
        }
        set_aside.extend(part.set_aside);
    }

    if let Some(p) = out.iter_mut().find(|r| r.kind == Kind::Protected) {
        for it in set_aside {
            p.size += it.size;
            p.count += 1;
            p.items.push(it);
        }
    }
    for r in out.iter_mut().filter(|r| !r.whole()) {
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
