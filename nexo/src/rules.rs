//! Reglas de limpieza: qué se puede borrar, qué va a la Papelera y qué está protegido.
//!
//! Regla de oro: los videos y documentos NUNCA se eliminan de forma definitiva ni
//! automática. Si aparecen dentro de una carpeta de basura, se apartan a la lista
//! "Videos y documentos" para que el usuario decida, y aun así van a la Papelera.

use std::path::{Path, PathBuf};

// Carpetas de basura y categorías propias de cada sistema.
#[cfg(windows)]
#[path = "rules_win.rs"]
mod plat;
#[cfg(unix)]
#[path = "unix/rules.rs"]
mod plat;

pub use plat::{categories, system_clean, system_size};


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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Protect {
    No,
    Video,
    Document,
}

pub fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

pub fn protection(p: &Path) -> Protect {
    let e = ext_of(p);
    if VIDEO_EXT.contains(&e.as_str()) {
        Protect::Video
    } else if DOC_EXT.contains(&e.as_str()) {
        Protect::Document
    } else {
        Protect::No
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Basura real: se elimina definitivamente. Única clase que usa el modo automático.
    Safe,
    /// Archivos del usuario que suelen sobrar: se muestran y van a la Papelera.
    Review,
    /// Videos y documentos: solo si el usuario los marca uno a uno, y a la Papelera.
    Protected,
}

#[derive(Clone)]
pub enum Filter {
    All,
    #[cfg_attr(all(unix, not(target_os = "macos")), allow(dead_code))]
    Prefix(&'static str),
    Ext(&'static [&'static str]),
    ProtectedOnly,
}

#[derive(Clone)]
pub struct Target {
    pub root: PathBuf,
    pub recursive: bool,
    pub filter: Filter,
    /// Antigüedad mínima (por fecha de modificación) para tocar un archivo.
    pub min_age_h: u64,
    pub admin: bool,
    /// Si alguno de estos programas está abierto, se omite este destino.
    pub blocked_by: &'static [&'static str],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Special {
    None,
    RecycleBin,
    /// Basura que solo el sistema puede borrar (caché de paquetes, registros del sistema):
    /// se mide y se limpia con un comando del propio sistema, que pide la contraseña.
    #[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
    System(&'static str),
}

pub struct CategoryDef {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub kind: Kind,
    pub special: Special,
    pub default_on: bool,
    pub targets: Vec<Target>,
}

impl CategoryDef {
    pub fn admin_only(&self) -> bool {
        !self.targets.is_empty() && self.targets.iter().all(|t| t.admin)
    }
}

#[cfg_attr(unix, allow(dead_code))]
pub struct Env {
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

impl Env {
    pub fn detect() -> Env {
        plat::detect()
    }

    pub fn is_forbidden(&self, p: &Path) -> bool {
        self.forbidden.iter().any(|f| is_inside(p, f))
    }
}

fn norm(p: &Path) -> String {
    p.to_string_lossy().trim_end_matches(std::path::MAIN_SEPARATOR).to_lowercase()
}

pub fn is_inside(p: &Path, root: &Path) -> bool {
    let (s, r) = (norm(p), norm(root));
    s == r || s.starts_with(&(r + std::path::MAIN_SEPARATOR_STR))
}

pub(crate) fn t(root: PathBuf, recursive: bool, filter: Filter, min_age_h: u64) -> Target {
    Target { root, recursive, filter, min_age_h, admin: false, blocked_by: &[] }
}

#[cfg_attr(unix, allow(dead_code))]
pub(crate) fn admin(mut x: Target) -> Target {
    x.admin = true;
    x
}

pub(crate) fn blocked(mut x: Target, by: &'static [&'static str]) -> Target {
    x.blocked_by = by;
    x
}

/// Categorías que el modo automático puede usar (solo basura real).
pub fn auto_eligible(defs: &[CategoryDef]) -> Vec<&CategoryDef> {
    defs.iter().filter(|d| d.kind == Kind::Safe && d.special == Special::None).collect()
}
