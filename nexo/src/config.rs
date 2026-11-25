use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum JbFrequency {
    Daily,
    Weekly,
    Monthly,
}

impl JbFrequency {
    pub fn jb_label(self) -> &'static str {
        match self {
            JbFrequency::Daily => "Diario",
            JbFrequency::Weekly => "Semanal",
            JbFrequency::Monthly => "Mensual",
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct JbConfig {
    pub auto_enabled: bool,
    pub frequency: JbFrequency,
    pub hour: u8,
    pub auto_categories: Vec<String>,
    pub last_auto_run: Option<u64>,
    pub vigia_enabled: bool,
    pub ram_warn: u8,
    pub ram_critical: u8,
    pub auto_stabilize: bool,
    pub alert_leaks: bool,
    pub alert_cpu: bool,
    pub alert_disk: bool,
    pub alert_security: bool,
    pub prev_power_plan: Option<String>,
    pub defender_scan_secs: u64,
}

impl Default for JbConfig {
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

impl JbConfig {
    pub fn jb_load() -> JbConfig {
        std::fs::read_to_string(jb_data_dir().join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn jb_save(&self) {
        let d = jb_data_dir();
        let _ = std::fs::create_dir_all(&d);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(d.join("config.json"), s);
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct JbHistoryEntry {
    pub ts: u64,
    pub mode: String,
    pub freed: u64,
    pub files: u64,
    pub skipped: u64,
    pub recycled: u64,
    pub details: Vec<(String, u64)>,
    pub note: String,
    pub review_bytes: u64,
}

const HISTORY_MAX: usize = 300;

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct JbAlertEntry {
    pub ts: u64,
    pub kind: String,
    pub text: String,
}

pub fn jb_load_alerts() -> Vec<JbAlertEntry> {
    std::fs::read_to_string(jb_data_dir().join("avisos.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

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

pub fn jb_config_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(jb_data_dir().join("config.json")).and_then(|m| m.modified()).ok()
}

pub fn jb_load_history() -> Vec<JbHistoryEntry> {
    std::fs::read_to_string(jb_data_dir().join("historial.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

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
