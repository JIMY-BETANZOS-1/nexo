use std::path::{Path, PathBuf};

#[cfg(windows)]
#[path = "rules_win.rs"]
mod plat;
#[cfg(unix)]
#[path = "unix/rules.rs"]
mod plat;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JbProtect {
    No,
    Video,
    Document,
}

pub fn jb_ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JbKind {
    Safe,
    Review,
    Protected,
}

#[derive(Clone)]
pub enum JbFilter {
    All,
    #[cfg_attr(all(unix, not(target_os = "macos")), allow(dead_code))]
    Prefix(&'static str),
    Ext(&'static [&'static str]),
    ProtectedOnly,
}

#[derive(Clone)]
pub struct JbTarget {
    pub root: PathBuf,
    pub recursive: bool,
    pub filter: JbFilter,
    pub min_age_h: u64,
    pub admin: bool,
    pub blocked_by: &'static [&'static str],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JbSpecial {
    None,
    RecycleBin,
    #[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
    System(&'static str),
}

pub struct JbCategoryDef {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub kind: JbKind,
    pub special: JbSpecial,
    pub default_on: bool,
    pub targets: Vec<JbTarget>,
}

impl JbCategoryDef {
    pub fn jb_admin_only(&self) -> bool {
        !self.targets.is_empty() && self.targets.iter().all(|t| t.admin)
    }
}

#[cfg_attr(unix, allow(dead_code))]
pub struct JbEnv {
    pub local: PathBuf,
    pub roaming: PathBuf,
    pub temp: PathBuf,
    pub windir: PathBuf,
    pub programdata: PathBuf,
    pub downloads: Option<PathBuf>,
    pub screenshots: Option<PathBuf>,
    pub forbidden: Vec<PathBuf>,
}

impl JbEnv {
    pub fn jb_detect() -> JbEnv {
        plat::jb_detect()
    }

    pub fn jb_is_forbidden(&self, p: &Path) -> bool {
        self.forbidden.iter().any(|f| jb_is_inside(p, f))
    }
}

fn jb_norm(p: &Path) -> String {
    p.to_string_lossy().trim_end_matches(std::path::MAIN_SEPARATOR).to_lowercase()
}

pub fn jb_is_inside(p: &Path, root: &Path) -> bool {
    let (s, r) = (jb_norm(p), jb_norm(root));
    s == r || s.starts_with(&(r + std::path::MAIN_SEPARATOR_STR))
}

pub(crate) fn jb_t(root: PathBuf, recursive: bool, filter: JbFilter, min_age_h: u64) -> JbTarget {
    JbTarget { root, recursive, filter, min_age_h, admin: false, blocked_by: &[] }
}

#[cfg_attr(unix, allow(dead_code))]
pub(crate) fn jb_admin(mut x: JbTarget) -> JbTarget {
    x.admin = true;
    x
}

pub(crate) fn jb_blocked(mut x: JbTarget, by: &'static [&'static str]) -> JbTarget {
    x.blocked_by = by;
    x
}

pub fn jb_auto_eligible(defs: &[JbCategoryDef]) -> Vec<&JbCategoryDef> {
    defs.iter().filter(|d| d.kind == JbKind::Safe && d.special == JbSpecial::None).collect()
}
