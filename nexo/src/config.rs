use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
}

impl Frequency {
    pub fn label(self) -> &'static str {
        match self {
            Frequency::Daily => "Diario",
            Frequency::Weekly => "Semanal",
            Frequency::Monthly => "Mensual",
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub auto_enabled: bool,
    pub frequency: Frequency,
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

impl Default for Config {
    fn default() -> Self {
        Config {
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
            frequency: Frequency::Weekly,
            hour: 12,
            auto_categories: ["temp_user", "temp_win", "thumbs", "errors", "browsers", "apps", "shaders", "update", "logs"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            last_auto_run: None,
        }
    }
}

/// Carpeta de datos de NEXO: %APPDATA%/NEXO (Windows), ~/.config/nexo (Linux) o
/// ~/Library/Application Support/NEXO (macOS).
pub fn data_dir() -> PathBuf {
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

impl Config {
    pub fn load() -> Config {
        std::fs::read_to_string(data_dir().join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let d = data_dir();
        let _ = std::fs::create_dir_all(&d);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(d.join("config.json"), s);
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct HistoryEntry {
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

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct AlertEntry {
    pub ts: u64,
    /// "ram", "critica", "fuga", "cpu", "disco", "virtual", "estabilizada".
    pub kind: String,
    pub text: String,
}

pub fn load_alerts() -> Vec<AlertEntry> {
    std::fs::read_to_string(data_dir().join("avisos.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn push_alert(kind: &str, text: &str) {
    let mut a = load_alerts();
    a.push(AlertEntry { ts: crate::fmt::now(), kind: kind.into(), text: text.into() });
    if a.len() > 100 {
        let extra = a.len() - 100;
        a.drain(0..extra);
    }
    let d = data_dir();
    let _ = std::fs::create_dir_all(&d);
    if let Ok(s) = serde_json::to_string(&a) {
        let _ = std::fs::write(d.join("avisos.json"), s);
    }
}

pub fn config_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(data_dir().join("config.json")).and_then(|m| m.modified()).ok()
}

pub fn load_history() -> Vec<HistoryEntry> {
    std::fs::read_to_string(data_dir().join("historial.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn push_history(e: HistoryEntry) -> Vec<HistoryEntry> {
    let mut h = load_history();
    h.push(e);
    if h.len() > HISTORY_MAX {
        let extra = h.len() - HISTORY_MAX;
        h.drain(0..extra);
    }
    let d = data_dir();
    let _ = std::fs::create_dir_all(&d);
    if let Ok(s) = serde_json::to_string(&h) {
        let _ = std::fs::write(d.join("historial.json"), s);
    }
    h
}
