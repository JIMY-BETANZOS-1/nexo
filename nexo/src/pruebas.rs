// Proyecto: NEXO
// Archivo: pruebas.rs
// Autor: Chatta Betanzos, Jimy Alexander (jimy-betanzos)

//! Pruebas automatizadas de NEXO (`cargo test`).
//!
//! Trabajan solo dentro de carpetas temporales creadas por cada prueba: nunca tocan
//! archivos reales del usuario. Se agrupan por módulo: reglas, escaneo, limpieza,
//! diagnóstico, memoria, configuración y formato.

// jimy-betanzos - dependencia: biblioteca estándar de Rust (collections)
use std::collections::HashSet;
// jimy-betanzos - dependencia: biblioteca estándar de Rust (fs)
use std::fs;
// jimy-betanzos - dependencia: biblioteca estándar de Rust (path)
use std::path::{Path, PathBuf};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (sync)
use std::sync::atomic::AtomicBool;
// jimy-betanzos - dependencia: biblioteca estándar de Rust (sync)
use std::sync::{Arc, Mutex};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (time)
use std::time::{Duration, SystemTime};

// jimy-betanzos - dependencia: módulo propio config
use crate::config::{JbConfig, JbFrequency};
// jimy-betanzos - dependencia: módulo propio diag
use crate::diag::{self, JbAction, JbBattery, JbDefender, JbFirewall, JbReport};
// jimy-betanzos - dependencia: módulo propio procs
use crate::procs::JbAppGroup;
// jimy-betanzos - dependencia: módulo propio rules
use crate::rules::{self, JbCategoryDef, JbEnv, JbFilter, JbKind, JbProtect, JbSpecial, JbTarget};
// jimy-betanzos - dependencia: módulo propio scan
use crate::scan::{self, JbCatResult, JbCtx, JbFoundItem, JbProgress};
// jimy-betanzos - dependencia: módulo propio
use crate::{clean, fmt};

// utilidades de prueba

/// jimy-betanzos: Carpeta personal del usuario en cualquier sistema.
fn jb_home_dir() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap()
}

/// jimy-betanzos: Carpeta temporal única que se borra al terminar la prueba.
struct JbSandbox(PathBuf);

/// jimy-betanzos: Métodos de Sandbox.
impl JbSandbox {
    /// jimy-betanzos: Crea una carpeta temporal aislada para la prueba.
    fn jb_new(name: &str) -> JbSandbox {
        let p = std::env::temp_dir().join(format!(
            "nexo_prueba_{}_{}_{}",
            name,
            std::process::id(),
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        JbSandbox(p)
    }
    /// jimy-betanzos: Crea un archivo de prueba del tamaño indicado.
    fn jb_file(&self, rel: &str, bytes: usize) -> PathBuf {
        let p = self.0.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, vec![b'x'; bytes]).unwrap();
        p
    }
    /// jimy-betanzos: Crea un archivo de prueba con fecha antigua.
    fn jb_old_file(&self, rel: &str, bytes: usize, hours: u64) -> PathBuf {
        let p = self.jb_file(rel, bytes);
        let f = fs::OpenOptions::new().write(true).open(&p).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(hours * 3600)).unwrap();
        p
    }
}

/// jimy-betanzos: Implementación de Drop para Sandbox.
impl Drop for JbSandbox {
    /// jimy-betanzos: Borra la carpeta temporal al terminar la prueba.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// jimy-betanzos: Arma un destino de escaneo para la prueba.
fn jb_target(root: &Path, recursive: bool, filter: JbFilter, min_age_h: u64) -> JbTarget {
    JbTarget { root: root.to_path_buf(), recursive, filter, min_age_h, admin: false, blocked_by: &[] }
}

/// jimy-betanzos: Arma un entorno de carpetas falso dentro de la carpeta temporal.
fn jb_env_in(sb: &JbSandbox) -> JbEnv {
    JbEnv {
        local: sb.0.join("local"),
        roaming: sb.0.join("roaming"),
        temp: sb.0.join("temp"),
        windir: sb.0.join("windows"),
        programdata: sb.0.join("programdata"),
        downloads: None,
        screenshots: None,
        forbidden: vec![sb.0.join("Documentos")],
    }
}

/// jimy-betanzos: Lista los archivos que el escaneo recorrería.
fn jb_walked(t: &JbTarget) -> Vec<PathBuf> {
    let cancel = AtomicBool::new(false);
    let mut v = Vec::new();
    scan::jb_walk(t, &cancel, |p, _| v.push(p.to_path_buf()), |_, _| {});
    v.sort();
    v
}

/// jimy-betanzos: Arma una categoría de basura segura para la prueba.
fn jb_safe_cat(id: &'static str, t: JbTarget) -> JbCategoryDef {
    JbCategoryDef { id, name: "Basura de prueba", desc: "", kind: JbKind::Safe, special: JbSpecial::None, default_on: true, targets: vec![t] }
}

/// jimy-betanzos: Arma la categoría de videos y documentos para la prueba.
fn jb_protected_cat() -> JbCategoryDef {
    JbCategoryDef { id: "protected", name: "Videos y documentos", desc: "", kind: JbKind::Protected, special: JbSpecial::None, default_on: false, targets: vec![] }
}

/// jimy-betanzos: Ejecuta un escaneo dentro de la prueba.
fn jb_run_scan(defs: &[JbCategoryDef]) -> Vec<JbCatResult> {
    let procs = HashSet::new();
    let progress = Arc::new(Mutex::new(JbProgress::default()));
    let cancel = AtomicBool::new(false);
    let ctx = JbCtx { admin: false, procs: &procs, progress: &progress, cancel: &cancel };
    scan::jb_scan_all(defs, None, &ctx)
}

/// jimy-betanzos: Ejecuta una limpieza dentro de la prueba.
fn jb_run_clean(defs: &[JbCategoryDef], res: &[JbCatResult], env: &JbEnv) -> clean::JbCleanReport {
    let procs = HashSet::new();
    let progress = Arc::new(Mutex::new(JbProgress::default()));
    let cancel = AtomicBool::new(false);
    let ctx = JbCtx { admin: false, procs: &procs, progress: &progress, cancel: &cancel };
    clean::jb_clean(defs, res, env, &ctx)
}

/// jimy-betanzos: Diagnóstico de un equipo sano para comparar.
fn jb_healthy_report() -> JbReport {
    let mut r = JbReport::default();
    r.raw.defender = JbDefender { available: true, enabled: true, realtime: true, sig_age: 0, quick_age: 0, tamper: true };
    r.raw.firewall = vec![
        JbFirewall { name: "Domain".into(), enabled: true },
        JbFirewall { name: "Private".into(), enabled: true },
        JbFirewall { name: "Public".into(), enabled: true },
    ];
    r.raw.uac = Some(true);
    r.raw.encryption = 1;
    r.raw.uptime_days = 1.0;
    r.disk_total = 500;
    r.disk_free = 300;
    r.ram_load = 40;
    r.commit = 40.0;
    r.power_guid = diag::BALANCED.into();
    r
}

/// jimy-betanzos: Nota de un área dentro del diagnóstico.
fn jb_area(r: &JbReport, name: &str) -> Option<u8> {
    r.areas.iter().find(|(a, _)| a == name).and_then(|(_, v)| *v)
}

// reglas (rules.rs)

/// jimy-betanzos: Prueba automática de reglas: videos se protegen.
#[test]
fn jb_reglas_videos_se_protegen() {
    for e in ["mp4", "MKV", "avi", "mov", "webm", "3gp"] {
        assert_eq!(rules::jb_protection(Path::new(&format!("C:\\x\\clip.{e}"))), JbProtect::Video, "{e}");
    }
}

/// jimy-betanzos: Prueba automática de reglas: documentos se protegen.
#[test]
fn jb_reglas_documentos_se_protegen() {
    for e in ["pdf", "DOCX", "xlsx", "pptx", "txt", "odt", "csv"] {
        assert_eq!(rules::jb_protection(Path::new(&format!("C:\\x\\informe.{e}"))), JbProtect::Document, "{e}");
    }
}

/// jimy-betanzos: Prueba automática de reglas: basura no esta protegida.
#[test]
fn jb_reglas_basura_no_esta_protegida() {
    for e in ["tmp", "log", "dmp", "etl", "png", "cache"] {
        assert_eq!(rules::jb_protection(Path::new(&format!("C:\\x\\a.{e}"))), JbProtect::No, "{e}");
    }
    assert_eq!(rules::jb_protection(Path::new("C:\\x\\sin_extension")), JbProtect::No);
}

/// jimy-betanzos: Prueba automática de reglas: extension en minusculas.
#[test]
fn jb_reglas_extension_en_minusculas() {
    assert_eq!(rules::jb_ext_of(Path::new("C:\\A\\B.Mp4")), "mp4");
    assert_eq!(rules::jb_ext_of(Path::new("C:\\A\\B")), "");
}

/// jimy-betanzos: Prueba automática de reglas: is inside reconoce subcarpetas.
#[test]
fn jb_reglas_is_inside_reconoce_subcarpetas() {
    let docs = jb_home_dir().join("Ana").join("Documents");
    assert!(rules::jb_is_inside(&docs.join("tesis.docx"), &docs));
    assert!(rules::jb_is_inside(&docs, &docs));
    // Sin distinguir mayúsculas (Windows y macOS no las distinguen).
    let upper = PathBuf::from(docs.to_string_lossy().to_uppercase());
    assert!(rules::jb_is_inside(&upper.join("tesis.docx"), &docs));
}

/// jimy-betanzos: Prueba automática de reglas: is inside no confunde prefijos.
#[test]
fn jb_reglas_is_inside_no_confunde_prefijos() {
    // «Documents2» no está dentro de «Documents».
    let base = jb_home_dir().join("Ana");
    assert!(!rules::jb_is_inside(&base.join("Documents2").join("a.txt"), &base.join("Documents")));
}

/// jimy-betanzos: Prueba automática de reglas: carpetas personales prohibidas.
#[test]
fn jb_reglas_carpetas_personales_prohibidas() {
    let sb = JbSandbox::jb_new("prohibidas");
    let env = jb_env_in(&sb);
    assert!(env.jb_is_forbidden(&sb.0.join("Documentos").join("cv.pdf")));
    assert!(!env.jb_is_forbidden(&sb.0.join("temp").join("a.tmp")));
}

/// jimy-betanzos: Prueba automática de reglas: entorno real protege documentos y videos.
#[test]
fn jb_reglas_entorno_real_protege_documentos_y_videos() {
    let env = JbEnv::jb_detect();
    let profile = jb_home_dir();
    assert!(env.jb_is_forbidden(&profile.join("Documents").join("x.docx")));
    assert!(env.jb_is_forbidden(&profile.join(if cfg!(target_os = "macos") { "Movies" } else { "Videos" }).join("x.mp4")));
    assert!(env.jb_is_forbidden(&profile.join("Desktop").join("x.txt")));
}

/// jimy-betanzos: Prueba automática de reglas: ninguna categoria segura apunta a carpetas personales.
#[test]
fn jb_reglas_ninguna_categoria_segura_apunta_a_carpetas_personales() {
    let env = JbEnv::jb_detect();
    for d in rules::jb_categories(&env).iter().filter(|d| d.kind == JbKind::Safe) {
        for t in &d.targets {
            assert!(!env.jb_is_forbidden(&t.root), "{} apunta a {}", d.id, t.root.display());
        }
    }
}

/// jimy-betanzos: Prueba automática de reglas: automatico solo usa categorias seguras.
#[test]
fn jb_reglas_automatico_solo_usa_categorias_seguras() {
    let env = JbEnv::jb_detect();
    let defs = rules::jb_categories(&env);
    let auto = rules::jb_auto_eligible(&defs);
    assert!(!auto.is_empty());
    assert!(auto.iter().all(|d| d.kind == JbKind::Safe));
    assert!(auto.iter().all(|d| d.id != "protected" && d.id != "screenshots" && d.id != "downloads_old"));
}

/// jimy-betanzos: Prueba automática de reglas: revision y protegidos empiezan desmarcados.
#[test]
fn jb_reglas_revision_y_protegidos_empiezan_desmarcados() {
    let env = JbEnv::jb_detect();
    for d in rules::jb_categories(&env).iter().filter(|d| d.kind != JbKind::Safe) {
        assert!(!d.default_on, "{} no debe venir marcada", d.id);
    }
}

/// jimy-betanzos: Prueba automática de reglas: existe categoria videos y documentos.
#[test]
fn jb_reglas_existe_categoria_videos_y_documentos() {
    let env = JbEnv::jb_detect();
    let defs = rules::jb_categories(&env);
    let p = defs.iter().find(|d| d.id == "protected").expect("falta la categoría protegida");
    assert_eq!(p.kind, JbKind::Protected);
}

/// jimy-betanzos: Prueba automática de reglas: descargas viejas no entra en subcarpetas.
#[test]
fn jb_reglas_descargas_viejas_no_entra_en_subcarpetas() {
    let env = JbEnv::jb_detect();
    if let Some(d) = rules::jb_categories(&env).iter().find(|d| d.id == "downloads_old") {
        assert!(d.targets.iter().all(|t| !t.recursive && t.min_age_h >= 24 * 30));
    }
}

/// jimy-betanzos: Prueba automática de reglas: temporales respetan 24 horas.
#[test]
fn jb_reglas_temporales_respetan_24_horas() {
    let env = JbEnv::jb_detect();
    let defs = rules::jb_categories(&env);
    let t = defs.iter().find(|d| d.id == "temp_user").unwrap();
    assert!(t.targets.iter().all(|t| t.min_age_h >= 24));
}

// escaneo (scan.rs)

/// jimy-betanzos: Prueba automática de escaneo: encuentra archivos de la carpeta.
#[test]
fn jb_escaneo_encuentra_archivos_de_la_carpeta() {
    let sb = JbSandbox::jb_new("walk");
    sb.jb_file("a.tmp", 10);
    sb.jb_file("sub/b.log", 20);
    assert_eq!(jb_walked(&jb_target(&sb.0, true, JbFilter::All, 0)).len(), 2);
}

/// jimy-betanzos: Prueba automática de escaneo: no recursivo ignora subcarpetas.
#[test]
fn jb_escaneo_no_recursivo_ignora_subcarpetas() {
    let sb = JbSandbox::jb_new("norec");
    sb.jb_file("a.tmp", 10);
    sb.jb_file("sub/b.tmp", 10);
    assert_eq!(jb_walked(&jb_target(&sb.0, false, JbFilter::All, 0)).len(), 1);
}

/// jimy-betanzos: Prueba automática de escaneo: respeta antiguedad minima.
#[test]
fn jb_escaneo_respeta_antiguedad_minima() {
    let sb = JbSandbox::jb_new("edad");
    sb.jb_file("nuevo.tmp", 10);
    sb.jb_old_file("viejo.tmp", 10, 48);
    let found = jb_walked(&jb_target(&sb.0, true, JbFilter::All, 24));
    assert_eq!(found.len(), 1);
    assert!(found[0].ends_with("viejo.tmp"));
}

/// jimy-betanzos: Prueba automática de escaneo: filtra por extension.
#[test]
fn jb_escaneo_filtra_por_extension() {
    let sb = JbSandbox::jb_new("ext");
    sb.jb_file("a.log", 1);
    sb.jb_file("b.etl", 1);
    sb.jb_file("c.txt", 1);
    assert_eq!(jb_walked(&jb_target(&sb.0, true, JbFilter::Ext(&["log", "etl"]), 0)).len(), 2);
}

/// jimy-betanzos: Prueba automática de escaneo: filtra por prefijo.
#[test]
fn jb_escaneo_filtra_por_prefijo() {
    let sb = JbSandbox::jb_new("prefijo");
    sb.jb_file("thumbcache_256.db", 1);
    sb.jb_file("iconcache_32.db", 1);
    let f = jb_walked(&jb_target(&sb.0, false, JbFilter::Prefix("thumbcache_"), 0));
    assert_eq!(f.len(), 1);
}

/// jimy-betanzos: Prueba automática de escaneo: filtro protegidos solo videos y documentos.
#[test]
fn jb_escaneo_filtro_protegidos_solo_videos_y_documentos() {
    let sb = JbSandbox::jb_new("prot");
    sb.jb_file("peli.mp4", 1);
    sb.jb_file("tesis.pdf", 1);
    sb.jb_file("setup.exe", 1);
    assert_eq!(jb_walked(&jb_target(&sb.0, true, JbFilter::ProtectedOnly, 0)).len(), 2);
}

/// jimy-betanzos: Prueba automática de escaneo: cancelado no recorre.
#[test]
fn jb_escaneo_cancelado_no_recorre() {
    let sb = JbSandbox::jb_new("cancel");
    sb.jb_file("a.tmp", 1);
    let cancel = AtomicBool::new(true);
    let mut n = 0;
    scan::jb_walk(&jb_target(&sb.0, true, JbFilter::All, 0), &cancel, |_, _| n += 1, |_, _| {});
    assert_eq!(n, 0);
}

/// jimy-betanzos: Prueba automática de escaneo: division no recursiva es una sola tarea.
#[test]
fn jb_escaneo_division_no_recursiva_es_una_sola_tarea() {
    let sb = JbSandbox::jb_new("split1");
    sb.jb_file("x/a.tmp", 1);
    assert_eq!(scan::jb_split_target(&jb_target(&sb.0, false, JbFilter::All, 0)).len(), 1);
}

/// jimy-betanzos: Prueba automática de escaneo: division crea una tarea por subcarpeta.
#[test]
fn jb_escaneo_division_crea_una_tarea_por_subcarpeta() {
    let sb = JbSandbox::jb_new("split2");
    sb.jb_file("a/1.tmp", 1);
    sb.jb_file("b/2.tmp", 1);
    sb.jb_file("c/3.tmp", 1);
    sb.jb_file("suelto.tmp", 1);
    let parts = scan::jb_split_target(&jb_target(&sb.0, true, JbFilter::All, 0));
    assert_eq!(parts.len(), 4); // raíz (no recursiva) + 3 subcarpetas
    assert!(!parts[0].recursive);
}

/// jimy-betanzos: Prueba automática de escaneo: division encuentra lo mismo que sin dividir.
#[test]
fn jb_escaneo_division_encuentra_lo_mismo_que_sin_dividir() {
    let sb = JbSandbox::jb_new("split3");
    for i in 0..6 {
        sb.jb_file(&format!("d{}/e/f{}.tmp", i % 3, i), 5);
    }
    sb.jb_file("raiz.tmp", 5);
    let t = jb_target(&sb.0, true, JbFilter::All, 0);
    let mut joined: Vec<PathBuf> = scan::jb_split_target(&t).iter().flat_map(jb_walked).collect();
    joined.sort();
    assert_eq!(joined, jb_walked(&t));
}

/// jimy-betanzos: Prueba automática de escaneo: omite destinos con programa abierto.
#[test]
fn jb_escaneo_omite_destinos_con_programa_abierto() {
    let mut t = jb_target(Path::new("C:\\x"), true, JbFilter::All, 0);
    t.blocked_by = &["chrome.exe"];
    let mut procs = HashSet::new();
    assert_eq!(scan::jb_blocked_by(&t, &procs), None);
    procs.insert("chrome.exe".to_string());
    assert_eq!(scan::jb_blocked_by(&t, &procs), Some("chrome.exe"));
}

/// jimy-betanzos: Prueba automática de escaneo: aparta videos y documentos de la basura.
#[test]
fn jb_escaneo_aparta_videos_y_documentos_de_la_basura() {
    let sb = JbSandbox::jb_new("aparta");
    let basura = sb.0.join("basura");
    sb.jb_file("basura/a.tmp", 100);
    sb.jb_file("basura/b.log", 50);
    sb.jb_file("basura/video.mp4", 1000);
    sb.jb_file("basura/notas.docx", 300);
    let defs = vec![jb_safe_cat("prueba", jb_target(&basura, true, JbFilter::All, 0)), jb_protected_cat()];
    let res = jb_run_scan(&defs);
    assert_eq!(res[0].count, 2);
    assert_eq!(res[0].size, 150);
    assert_eq!(res[1].count, 2, "el video y el documento pasan a «Videos y documentos»");
    assert!(res[1].items.iter().all(|i| !i.selected), "los protegidos nunca vienen marcados");
}

/// jimy-betanzos: Prueba automática de escaneo: tamano seleccionado por bloque y por archivo.
#[test]
fn jb_escaneo_tamano_seleccionado_por_bloque_y_por_archivo() {
    let item = |s| JbFoundItem { path: PathBuf::from("x"), size: s, selected: s > 10, origin: "" };
    let mut r = JbCatResult {
        id: "x", name: "x", desc: "", kind: JbKind::Review, special: JbSpecial::None, admin_only: false,
        size: 0, count: 0, items: vec![item(5), item(20), item(30)], selected: false, notes: vec![],
        expanded: false, preview: None,
    };
    assert_eq!(r.jb_selected_size(), 50);
    assert_eq!(r.jb_selected_count(), 2);
    r.kind = JbKind::Safe;
    r.size = 999;
    r.count = 7;
    assert_eq!(r.jb_selected_size(), 0);
    r.selected = true;
    assert_eq!(r.jb_selected_size(), 999);
}

/// jimy-betanzos: Prueba automática de escaneo: hilos entre 2 y 8.
#[test]
fn jb_escaneo_hilos_entre_2_y_8() {
    let w = scan::jb_workers();
    assert!((2..=8).contains(&w));
}

// limpieza (clean.rs)

/// jimy-betanzos: Prueba automática de limpieza: borra basura y conserva videos y documentos.
#[test]
fn jb_limpieza_borra_basura_y_conserva_videos_y_documentos() {
    let sb = JbSandbox::jb_new("limpia");
    let env = jb_env_in(&sb);
    let basura = sb.0.join("temp");
    let tmp = sb.jb_file("temp/a.tmp", 100);
    let log = sb.jb_file("temp/sub/b.log", 50);
    let mp4 = sb.jb_file("temp/sub/video.mp4", 1000);
    let pdf = sb.jb_file("temp/informe.pdf", 300);
    let defs = vec![jb_safe_cat("prueba", jb_target(&basura, true, JbFilter::All, 0)), jb_protected_cat()];
    let res = jb_run_scan(&defs);
    let rep = jb_run_clean(&defs, &res, &env);
    assert!(!tmp.exists() && !log.exists(), "la basura se elimina");
    assert!(mp4.exists() && pdf.exists(), "videos y documentos quedan intactos");
    assert_eq!(rep.freed, 150);
    assert_eq!(rep.files, 2);
    assert_eq!(rep.recycled, 0);
}

/// jimy-betanzos: Prueba automática de limpieza: nunca borra en carpetas personales.
#[test]
fn jb_limpieza_nunca_borra_en_carpetas_personales() {
    let sb = JbSandbox::jb_new("personal");
    let env = jb_env_in(&sb);
    // Aunque una regla apuntara por error a «Documentos», el doble seguro lo impide.
    let f = sb.jb_file("Documentos/temporal.tmp", 10);
    let defs = vec![jb_safe_cat("error", jb_target(&sb.0.join("Documentos"), true, JbFilter::All, 0))];
    let res = jb_run_scan(&defs);
    let rep = jb_run_clean(&defs, &res, &env);
    assert!(f.exists());
    assert_eq!(rep.files, 0);
    assert_eq!(rep.skipped, 1);
}

/// jimy-betanzos: Prueba automática de limpieza: respeta categoria desmarcada.
#[test]
fn jb_limpieza_respeta_categoria_desmarcada() {
    let sb = JbSandbox::jb_new("desmarcada");
    let env = jb_env_in(&sb);
    let f = sb.jb_file("temp/a.tmp", 10);
    let defs = vec![jb_safe_cat("prueba", jb_target(&sb.0.join("temp"), true, JbFilter::All, 0))];
    let mut res = jb_run_scan(&defs);
    res[0].selected = false;
    let rep = jb_run_clean(&defs, &res, &env);
    assert!(f.exists());
    assert_eq!(rep.files, 0);
}

/// jimy-betanzos: Prueba automática de limpieza: respeta antiguedad.
#[test]
fn jb_limpieza_respeta_antiguedad() {
    let sb = JbSandbox::jb_new("edadlimpia");
    let env = jb_env_in(&sb);
    let nuevo = sb.jb_file("temp/nuevo.tmp", 10);
    let viejo = sb.jb_old_file("temp/viejo.tmp", 10, 30);
    let defs = vec![jb_safe_cat("prueba", jb_target(&sb.0.join("temp"), true, JbFilter::All, 24))];
    let res = jb_run_scan(&defs);
    jb_run_clean(&defs, &res, &env);
    assert!(nuevo.exists());
    assert!(!viejo.exists());
}

/// jimy-betanzos: Prueba automática de limpieza: borra archivos de solo lectura.
#[test]
fn jb_limpieza_borra_archivos_de_solo_lectura() {
    let sb = JbSandbox::jb_new("solo_lectura");
    let env = jb_env_in(&sb);
    let f = sb.jb_file("temp/ro.tmp", 10);
    let mut p = fs::metadata(&f).unwrap().permissions();
    p.set_readonly(true);
    fs::set_permissions(&f, p).unwrap();
    let defs = vec![jb_safe_cat("prueba", jb_target(&sb.0.join("temp"), true, JbFilter::All, 0))];
    let res = jb_run_scan(&defs);
    jb_run_clean(&defs, &res, &env);
    assert!(!f.exists());
}

// diagnóstico (diag.rs)

/// jimy-betanzos: Prueba automática de diagnóstico: equipo sano tiene 100.
#[test]
fn jb_diagnostico_equipo_sano_tiene_100() {
    let mut r = jb_healthy_report();
    diag::jb_analyze(&mut r);
    assert_eq!(jb_area(&r, "Seguridad"), Some(100));
    assert_eq!(r.overall, 100);
    assert!(r.recs.is_empty());
}

/// jimy-betanzos: Prueba automática de diagnóstico: consejos no restan puntos.
#[test]
fn jb_diagnostico_consejos_no_restan_puntos() {
    let mut r = jb_healthy_report();
    r.raw.encryption = 2; // disco sin cifrar: solo es un consejo
    diag::jb_analyze(&mut r);
    assert_eq!(r.recs.len(), 1);
    assert_eq!(r.recs[0].severity, 0);
    assert_eq!(jb_area(&r, "Seguridad"), Some(100));
}

/// jimy-betanzos: Prueba automática de diagnóstico: proteccion en tiempo real apagada es importante.
#[test]
fn jb_diagnostico_proteccion_en_tiempo_real_apagada_es_importante() {
    let mut r = jb_healthy_report();
    r.raw.defender.realtime = false;
    diag::jb_analyze(&mut r);
    assert_eq!(r.recs[0].severity, 2);
    assert_eq!(jb_area(&r, "Seguridad"), Some(65));
}

/// jimy-betanzos: Prueba automática de diagnóstico: firewall apagado resta.
#[test]
fn jb_diagnostico_firewall_apagado_resta() {
    let mut r = jb_healthy_report();
    r.raw.firewall[2].enabled = false;
    diag::jb_analyze(&mut r);
    assert!(r.recs.iter().any(|x| x.title.contains("Public")));
    assert_eq!(jb_area(&r, "Seguridad"), Some(65));
}

/// jimy-betanzos: Prueba automática de diagnóstico: analisis viejo propone analizar.
#[test]
fn jb_diagnostico_analisis_viejo_propone_analizar() {
    let mut r = jb_healthy_report();
    r.raw.defender.quick_age = 30;
    diag::jb_analyze(&mut r);
    if diag::jb_quick_scan_available() {
        assert_eq!(r.recs[0].action, Some(JbAction::QuickScan));
        assert_eq!(jb_area(&r, "Seguridad"), Some(85));
    } else {
        // Sin herramienta de análisis manual (macOS, o Linux sin ClamAV) no se recomienda.
        assert!(r.recs.is_empty());
        assert_eq!(jb_area(&r, "Seguridad"), Some(100));
    }
}

/// jimy-betanzos: Prueba automática de diagnóstico: disco casi lleno lleva a limpieza.
#[test]
fn jb_diagnostico_disco_casi_lleno_lleva_a_limpieza() {
    let mut r = jb_healthy_report();
    r.disk_free = 50; // 10 %
    diag::jb_analyze(&mut r);
    let rec = r.recs.iter().find(|x| x.area == "Almacenamiento").unwrap();
    assert_eq!(rec.severity, 2);
    assert_eq!(rec.action, Some(JbAction::GoClean));
}

/// jimy-betanzos: Prueba automática de diagnóstico: plan de energia distinto sugiere equilibrado.
#[test]
fn jb_diagnostico_plan_de_energia_distinto_sugiere_equilibrado() {
    let mut r = jb_healthy_report();
    r.power_guid = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".into();
    r.power_name = "Alto rendimiento".into();
    diag::jb_analyze(&mut r);
    assert!(r.recs.iter().any(|x| x.action == Some(JbAction::SetBalanced)));
}

/// jimy-betanzos: Prueba automática de diagnóstico: recomendaciones ordenadas por gravedad.
#[test]
fn jb_diagnostico_recomendaciones_ordenadas_por_gravedad() {
    let mut r = jb_healthy_report();
    r.raw.encryption = 2;
    r.raw.rdp = true;
    r.raw.uac = Some(false);
    diag::jb_analyze(&mut r);
    let sev: Vec<u8> = r.recs.iter().map(|x| x.severity).collect();
    let mut sorted = sev.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(sev, sorted);
}

/// jimy-betanzos: Prueba automática de diagnóstico: salud de bateria.
#[test]
fn jb_diagnostico_salud_de_bateria() {
    assert_eq!(JbBattery { design: 0, full: 0, cycles: 0 }.jb_health(), None);
    assert_eq!(JbBattery { design: 60000, full: 45000, cycles: 100 }.jb_health(), Some(75.0));
    assert_eq!(JbBattery { design: 50000, full: 52000, cycles: 1 }.jb_health(), Some(100.0));
}

/// jimy-betanzos: Prueba automática de diagnóstico: bateria desgastada resta en su area.
#[test]
fn jb_diagnostico_bateria_desgastada_resta_en_su_area() {
    let mut r = jb_healthy_report();
    r.raw.battery = JbBattery { design: 60000, full: 42000, cycles: 300 }; // 70 %
    diag::jb_analyze(&mut r);
    assert_eq!(jb_area(&r, "Batería"), Some(55)); // 70 − 15
}

/// jimy-betanzos: Prueba automática de diagnóstico: hosts conocidos no alertan.
#[test]
fn jb_diagnostico_hosts_conocidos_no_alertan() {
    let hosts: Vec<String> = [
        "127.0.0.1 kubernetes.docker.internal",
        "192.168.1.5 host.docker.internal",
        "127.0.0.1 miapp.test",
        "0.0.0.0 anuncios.com",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert!(diag::jb_unknown_hosts(&hosts).is_empty());
}

/// jimy-betanzos: Prueba automática de diagnóstico: hosts desconocidos si alertan.
#[test]
fn jb_diagnostico_hosts_desconocidos_si_alertan() {
    let hosts = vec!["203.0.113.9 www.mibanco.com".to_string()];
    assert_eq!(diag::jb_unknown_hosts(&hosts).len(), 1);
}

/// jimy-betanzos: Prueba automática de diagnóstico: calificacion por rangos.
#[test]
fn jb_diagnostico_calificacion_por_rangos() {
    assert_eq!(diag::jb_grade(100), "Excelente");
    assert_eq!(diag::jb_grade(90), "Excelente");
    assert_eq!(diag::jb_grade(89), "Bueno");
    assert_eq!(diag::jb_grade(74), "Mejorable");
    assert_eq!(diag::jb_grade(10), "Necesita atención");
}

// memoria (procs.rs)

/// jimy-betanzos: Programa de ejemplo para las pruebas de memoria.
fn jb_app() -> JbAppGroup {
    JbAppGroup { exe: "app.exe".into(), name: "App".into(), accessible: true, ..Default::default() }
}

/// jimy-betanzos: Prueba automática de memoria: compacta solo apps inactivas de fondo.
#[test]
fn jb_memoria_compacta_solo_apps_inactivas_de_fondo() {
    // Solo Windows compacta programas uno por uno; Linux y macOS liberan la caché del sistema.
    assert_eq!(jb_app().jb_can_trim(), cfg!(windows));
    assert!(!JbAppGroup { system: true, ..jb_app() }.jb_can_trim());
    assert!(!JbAppGroup { foreground: true, ..jb_app() }.jb_can_trim());
    assert!(!JbAppGroup { installing: true, ..jb_app() }.jb_can_trim());
    assert!(!JbAppGroup { cpu_pct: 30.0, ..jb_app() }.jb_can_trim());
    assert!(!JbAppGroup { io_rate: 10.0 * 1024.0 * 1024.0, ..jb_app() }.jb_can_trim());
}

/// jimy-betanzos: Prueba automática de memoria: avisa cambios sin guardar.
#[test]
fn jb_memoria_avisa_cambios_sin_guardar() {
    let r = JbAppGroup { unsaved: true, editor: true, ..jb_app() }.jb_reasons();
    assert!(r.iter().any(|x| x.contains("SIN GUARDAR")));
}

// configuración (config.rs)

/// jimy-betanzos: Prueba automática de configuración: por defecto segura.
#[test]
fn jb_configuracion_por_defecto_segura() {
    let c = JbConfig::default();
    assert!(!c.auto_enabled, "el modo automático empieza apagado");
    assert_eq!(c.frequency, JbFrequency::Weekly);
    assert!(c.ram_warn < c.ram_critical);
}

/// jimy-betanzos: Prueba automática de configuración: automatico por defecto solo basura segura.
#[test]
fn jb_configuracion_automatico_por_defecto_solo_basura_segura() {
    let env = JbEnv::jb_detect();
    let defs = rules::jb_categories(&env);
    for id in JbConfig::default().auto_categories {
        if let Some(d) = defs.iter().find(|d| d.id == id) {
            assert_eq!(d.kind, JbKind::Safe, "{id}");
        }
    }
}

/// jimy-betanzos: Prueba automática de configuración: json incompleto usa valores por defecto.
#[test]
fn jb_configuracion_json_incompleto_usa_valores_por_defecto() {
    let c: JbConfig = serde_json::from_str(r#"{"auto_enabled": true, "hour": 3}"#).unwrap();
    assert!(c.auto_enabled);
    assert_eq!(c.hour, 3);
    assert_eq!(c.ram_critical, 92);
}

/// jimy-betanzos: Prueba automática de configuración: frecuencias en espanol.
#[test]
fn jb_configuracion_frecuencias_en_espanol() {
    assert_eq!(JbFrequency::Daily.jb_label(), "Diario");
    assert_eq!(JbFrequency::Weekly.jb_label(), "Semanal");
    assert_eq!(JbFrequency::Monthly.jb_label(), "Mensual");
}

// formato (fmt.rs)

/// jimy-betanzos: Prueba automática de formato: bytes con coma decimal.
#[test]
fn jb_formato_bytes_con_coma_decimal() {
    assert_eq!(fmt::jb_bytes(0), "0 B");
    assert_eq!(fmt::jb_bytes(1536), "1,50 KB");
    assert_eq!(fmt::jb_bytes(15 * 1024 * 1024), "15,0 MB");
    assert_eq!(fmt::jb_bytes(300 * 1024 * 1024 * 1024), "300 GB");
}

/// jimy-betanzos: Prueba automática de formato: miles con punto.
#[test]
fn jb_formato_miles_con_punto() {
    assert_eq!(fmt::jb_count(999), "999");
    assert_eq!(fmt::jb_count(1_234_567), "1.234.567");
}

/// jimy-betanzos: Prueba automática de formato: duraciones.
#[test]
fn jb_formato_duraciones() {
    assert_eq!(fmt::jb_secs(8.4), "8 s");
    assert_eq!(fmt::jb_secs(75.0), "1 min 15 s");
    assert_eq!(fmt::jb_secs(3725.0), "1 h 02 min");
}

/// jimy-betanzos: Prueba automática de formato: ruta corta conserva el final.
#[test]
fn jb_formato_ruta_corta_conserva_el_final() {
    let s = fmt::jb_short_path(&jb_home_dir().join("una").join("ruta").join("muy").join("larga").join("archivo.tmp"), 15);
    assert!(s.starts_with('…') && s.ends_with("archivo.tmp"));
    assert_eq!(s.chars().count(), 15);
}

/// jimy-betanzos: Prueba automática de formato: tiempo relativo.
#[test]
fn jb_formato_tiempo_relativo() {
    let now = fmt::jb_now();
    assert_eq!(fmt::jb_ago(now), "hace un momento");
    assert_eq!(fmt::jb_ago(now - 120), "hace 2 min");
    assert_eq!(fmt::jb_ago(now - 86_400), "hace 1 día");
}

// imágenes (thumbs.rs)

/// jimy-betanzos: Prueba automática de imagen: bmp cabecera y colores.
#[test]
fn jb_imagen_bmp_cabecera_y_colores() {
    // 2×1 píxeles: rojo y verde opacos.
    let b = crate::thumbs::jb_bmp(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]);
    assert_eq!(&b[0..2], b"BM");
    assert_eq!(b.len(), 54 + 8);
    assert_eq!(u32::from_le_bytes(b[2..6].try_into().unwrap()), 62);
    assert_eq!(&b[54..58], &[0, 0, 255, 255]); // BGRA: rojo
    assert_eq!(&b[58..62], &[0, 255, 0, 255]); // BGRA: verde
}
