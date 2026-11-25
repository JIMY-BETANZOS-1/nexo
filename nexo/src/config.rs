// ════════════════════════════════════════════════════════════════════════
//  NEXO · nexo/src/config.rs
//  Autor: Chatta Betanzos, Jimy Alexander  ·  jimy-betanzos
// ════════════════════════════════════════════════════════════════════════

// jimy-betanzos · dependencia: biblioteca estándar de Rust (path)
use std::path::PathBuf;

// jimy-betanzos · dependencia: guardar y leer datos en JSON (serde)
use serde::{Deserialize, Serialize};

/// jimy-betanzos · Frecuencia de la limpieza automática: diaria, semanal o mensual.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum JbFrequency {
    Daily,
    Weekly,
    Monthly,
}

/// jimy-betanzos · Métodos de Frequency.
impl JbFrequency {
    /// jimy-betanzos · Nombre en español de la frecuencia.
    pub fn jb_label(self) -> &'static str {
        match self {
            JbFrequency::Daily => "Diario",
            JbFrequency::Weekly => "Semanal",
            JbFrequency::Monthly => "Mensual",
        }
    }
}

/// jimy-betanzos · Configuración del usuario guardada en config.json.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct JbConfig {
    pub auto_enabled: bool,
    pub frequency: JbFrequency,
    pub hour: u8,
    pub auto_categories: Vec<String>,
    pub last_auto_run: Option<u64>,
    // Vigía (fase 2)
    pub vigia_enabled: bool,
    pub ram_warn: u8,
    pub ram_critical: u8,
    pub auto_stabilize: bool,
    pub alert_leaks: bool,
    pub alert_cpu: bool,
    pub alert_disk: bool,
    // Protección (fase 3)
    pub alert_security: bool,
    /// Plan de energía anterior, para poder deshacer el cambio a «Equilibrado».
    pub prev_power_plan: Option<String>,
    pub defender_scan_secs: u64,
}

/// jimy-betanzos · Implementación de Default para Config.
impl Default for JbConfig {
    /// jimy-betanzos · Valores iniciales seguros de la configuración.
    fn default() -> Self {
        JbConfig {
            vigia_enabled: false,
            ram_warn: 85,
            ram_critical: 92,
            auto_stabilize: true,
            alert_leaks: true,
            alert_cpu: true,
            alert_disk: true,
            alert_security: true,
            prev_power_plan: None,
            defender_scan_secs: 0,
            auto_enabled: false,
            frequency: JbFrequency::Weekly,
            hour: 12,
            auto_categories: ["temp_user", "temp_win", "thumbs", "errors", "browsers", "apps", "shaders", "update", "logs"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            last_auto_run: None,
        }
    }
}

/// jimy-betanzos · Carpeta de datos de NEXO: %APPDATA%/NEXO (Windows), ~/.config/nexo (Linux) o
/// ~/Library/Application Support/NEXO (macOS).
pub fn jb_data_dir() -> PathBuf {
    #[cfg(windows)]
    let dir = std::env::var_os("APPDATA").map(|b| PathBuf::from(b).join("NEXO"));
    #[cfg(target_os = "macos")]
    let dir = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/NEXO"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|b| b.join("nexo"));
    dir.unwrap_or_else(|| std::env::temp_dir().join("NEXO"))
}

/// jimy-betanzos · Métodos de Config.
impl JbConfig {
    /// jimy-betanzos · Lee la configuración guardada (o usa la inicial).
    pub fn jb_load() -> JbConfig {
        std::fs::read_to_string(jb_data_dir().join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// jimy-betanzos · Guarda la configuración en disco.
    pub fn jb_save(&self) {
        let d = jb_data_dir();
        let _ = std::fs::create_dir_all(&d);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(d.join("config.json"), s);
        }
    }
}

/// jimy-betanzos · Registro de una limpieza en el historial.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct JbHistoryEntry {
    pub ts: u64,
    /// "manual", "auto" o "pospuesta".
    pub mode: String,
    pub freed: u64,
    pub files: u64,
    pub skipped: u64,
    pub recycled: u64,
    pub details: Vec<(String, u64)>,
    pub note: String,
    /// Bytes que quedaron "para revisar" (capturas, descargas, videos y documentos).
    pub review_bytes: u64,
}

const HISTORY_MAX: usize = 300;

/// jimy-betanzos · Aviso del vigía guardado para mostrarlo en la ventana.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct JbAlertEntry {
    pub ts: u64,
    /// "ram", "critica", "fuga", "cpu", "disco", "virtual", "estabilizada".
    pub kind: String,
    pub text: String,
}

/// jimy-betanzos · Lee los últimos avisos del vigía.
pub fn jb_load_alerts() -> Vec<JbAlertEntry> {
    std::fs::read_to_string(jb_data_dir().join("avisos.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// jimy-betanzos · Agrega un aviso del vigía (se guardan los más recientes).
pub fn jb_push_alert(kind: &str, text: &str) {
    let mut a = jb_load_alerts();
    a.push(JbAlertEntry { ts: crate::fmt::jb_now(), kind: kind.into(), text: text.into() });
    if a.len() > 100 {
        let extra = a.len() - 100;
        a.drain(0..extra);
    }
    let d = jb_data_dir();
    let _ = std::fs::create_dir_all(&d);
    if let Ok(s) = serde_json::to_string(&a) {
        let _ = std::fs::write(d.join("avisos.json"), s);
    }
}

/// jimy-betanzos · Fecha de la última modificación de la configuración.
pub fn jb_config_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(jb_data_dir().join("config.json")).and_then(|m| m.modified()).ok()
}

/// jimy-betanzos · Lee el historial de limpiezas.
pub fn jb_load_history() -> Vec<JbHistoryEntry> {
    std::fs::read_to_string(jb_data_dir().join("historial.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// jimy-betanzos · Agrega una limpieza al historial y lo guarda.
pub fn jb_push_history(e: JbHistoryEntry) -> Vec<JbHistoryEntry> {
    let mut h = jb_load_history();
    h.push(e);
    if h.len() > HISTORY_MAX {
        let extra = h.len() - HISTORY_MAX;
        h.drain(0..extra);
    }
    let d = jb_data_dir();
    let _ = std::fs::create_dir_all(&d);
    if let Ok(s) = serde_json::to_string(&h) {
        let _ = std::fs::write(d.join("historial.json"), s);
    }
    h
}
