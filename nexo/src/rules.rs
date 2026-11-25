// ════════════════════════════════════════════════════════════════════════
//  NEXO · nexo/src/rules.rs
//  Autor: Chatta Betanzos, Jimy Alexander  ·  jimy-betanzos
// ════════════════════════════════════════════════════════════════════════

//! Reglas de limpieza: qué se puede borrar, qué va a la Papelera y qué está protegido.
//!
//! Regla de oro: los videos y documentos NUNCA se eliminan de forma definitiva ni
//! automática. Si aparecen dentro de una carpeta de basura, se apartan a la lista
//! "Videos y documentos" para que el usuario decida, y aun así van a la Papelera.

// jimy-betanzos · dependencia: biblioteca estándar de Rust (path)
use std::path::{Path, PathBuf};

// Carpetas de basura y categorías propias de cada sistema.
#[cfg(windows)]
#[path = "rules_win.rs"]
mod plat;
#[cfg(unix)]
#[path = "unix/rules.rs"]
mod plat;

// jimy-betanzos · dependencia: módulo propio de la plataforma (Windows o Linux/macOS)
pub use plat::{jb_categories, jb_system_clean, jb_system_size};


pub const VIDEO_EXT: &[&str] = &[
    "mp4", "m4v", "mkv", "avi", "mov", "wmv", "flv", "webm", "3gp", "3g2", "mpg", "mpeg", "m2ts",
    "mts", "vob", "ogv", "rm", "rmvb", "asf", "divx", "f4v",
];

pub const DOC_EXT: &[&str] = &[
    "pdf", "doc", "docx", "docm", "dot", "dotx", "xls", "xlsx", "xlsm", "xlsb", "csv", "ppt",
    "pptx", "pptm", "pps", "ppsx", "odt", "ods", "odp", "odg", "rtf", "txt", "md", "epub", "mobi",
    "azw3", "pages", "numbers", "key", "one", "xps", "oxps", "tex", "djvu", "pub", "vsd", "vsdx",
    "accdb", "mdb",
];

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "webp", "heic", "tif", "tiff"];

#[cfg_attr(unix, allow(dead_code))]
pub const PACKAGE_EXT: &[&str] = &[
    "exe", "msi", "msix", "msixbundle", "appx", "appxbundle", "zip", "rar", "7z", "iso", "cab",
    "tar", "gz",
];

/// jimy-betanzos · Nivel de protección de un archivo (normal o protegido).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JbProtect {
    No,
    Video,
    Document,
}

/// jimy-betanzos · Extensión de un archivo en minúsculas.
pub fn jb_ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// jimy-betanzos · Decide si un archivo es video o documento (protegido).
pub fn jb_protection(p: &Path) -> JbProtect {
    let e = jb_ext_of(p);
    if VIDEO_EXT.contains(&e.as_str()) {
        JbProtect::Video
    } else if DOC_EXT.contains(&e.as_str()) {
        JbProtect::Document
    } else {
        JbProtect::No
    }
}

/// jimy-betanzos · Tipo de categoría: segura, para revisar o protegida.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JbKind {
    /// Basura real: se elimina definitivamente. Única clase que usa el modo automático.
    Safe,
    /// Archivos del usuario que suelen sobrar: se muestran y van a la Papelera.
    Review,
    /// Videos y documentos: solo si el usuario los marca uno a uno, y a la Papelera.
    Protected,
}

/// jimy-betanzos · Filtro de archivos de un destino (todos, por extensión o prefijo).
#[derive(Clone)]
pub enum JbFilter {
    All,
    #[cfg_attr(all(unix, not(target_os = "macos")), allow(dead_code))]
    Prefix(&'static str),
    Ext(&'static [&'static str]),
    ProtectedOnly,
}

/// jimy-betanzos · Carpeta que se escanea con su filtro y antigüedad mínima.
#[derive(Clone)]
pub struct JbTarget {
    pub root: PathBuf,
    pub recursive: bool,
    pub filter: JbFilter,
    /// Antigüedad mínima (por fecha de modificación) para tocar un archivo.
    pub min_age_h: u64,
    pub admin: bool,
    /// Si alguno de estos programas está abierto, se omite este destino.
    pub blocked_by: &'static [&'static str],
}

/// jimy-betanzos · Tratamiento especial de una categoría (Papelera, sistema…).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JbSpecial {
    None,
    RecycleBin,
    /// Basura que solo el sistema puede borrar (caché de paquetes, registros del sistema):
    /// se mide y se limpia con un comando del propio sistema, que pide la contraseña.
    #[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
    System(&'static str),
}

/// jimy-betanzos · Categoría de limpieza: nombre, descripción, tipo y destinos.
pub struct JbCategoryDef {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub kind: JbKind,
    pub special: JbSpecial,
    pub default_on: bool,
    pub targets: Vec<JbTarget>,
}

/// jimy-betanzos · Métodos de CategoryDef.
impl JbCategoryDef {
    /// jimy-betanzos · Indica si la categoría necesita permiso de administrador.
    pub fn jb_admin_only(&self) -> bool {
        !self.targets.is_empty() && self.targets.iter().all(|t| t.admin)
    }
}

/// jimy-betanzos · Carpetas del usuario y del sistema que usan las reglas.
#[cfg_attr(unix, allow(dead_code))]
pub struct JbEnv {
    pub local: PathBuf,
    pub roaming: PathBuf,
    pub temp: PathBuf,
    pub windir: PathBuf,
    pub programdata: PathBuf,
    pub downloads: Option<PathBuf>,
    pub screenshots: Option<PathBuf>,
    /// Carpetas personales donde jamás se borra nada de forma definitiva.
    pub forbidden: Vec<PathBuf>,
}

/// jimy-betanzos · Métodos de Env.
impl JbEnv {
    /// jimy-betanzos · Detecta las carpetas reales del equipo.
    pub fn jb_detect() -> JbEnv {
        plat::jb_detect()
    }

    /// jimy-betanzos · Indica si una ruta está dentro de una carpeta personal prohibida.
    pub fn jb_is_forbidden(&self, p: &Path) -> bool {
        self.forbidden.iter().any(|f| jb_is_inside(p, f))
    }
}

/// jimy-betanzos · Normaliza una ruta para compararla (minúsculas, sin separador final).
fn jb_norm(p: &Path) -> String {
    p.to_string_lossy().trim_end_matches(std::path::MAIN_SEPARATOR).to_lowercase()
}

// jimy-betanzos: se compara con el separador al final para que «/tmp» no coincida con «/tmpx».
pub fn jb_is_inside(p: &Path, root: &Path) -> bool {
    let (s, r) = (jb_norm(p), jb_norm(root));
    s == r || s.starts_with(&(r + std::path::MAIN_SEPARATOR_STR))
}

/// jimy-betanzos · Crea un destino de escaneo.
pub(crate) fn jb_t(root: PathBuf, recursive: bool, filter: JbFilter, min_age_h: u64) -> JbTarget {
    JbTarget { root, recursive, filter, min_age_h, admin: false, blocked_by: &[] }
}

/// jimy-betanzos · Marca un destino como de administrador.
#[cfg_attr(unix, allow(dead_code))]
pub(crate) fn jb_admin(mut x: JbTarget) -> JbTarget {
    x.admin = true;
    x
}

/// jimy-betanzos · Marca un destino que no se toca si cierto programa está abierto.
pub(crate) fn jb_blocked(mut x: JbTarget, by: &'static [&'static str]) -> JbTarget {
    x.blocked_by = by;
    x
}

// jimy-betanzos: el modo automático solo usa basura real (categorías seguras sin tratamiento especial).
// Videos, documentos, capturas y descargas quedan fuera siempre, aunque el usuario lo active.
/// jimy-betanzos · Categorías que el modo automático puede usar (solo basura real).
pub fn jb_auto_eligible(defs: &[JbCategoryDef]) -> Vec<&JbCategoryDef> {
    defs.iter().filter(|d| d.kind == JbKind::Safe && d.special == JbSpecial::None).collect()
}
