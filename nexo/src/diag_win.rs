//! Parte de Windows del diagnóstico: script de PowerShell, plan de energía, programas de
//! inicio (StartupApproved), Microsoft Defender e informe de batería.

use std::path::PathBuf;

use windows_sys::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_BINARY};

use super::{Progress, Raw, StartupItem};
use crate::{fmt, sys};

const SCRIPT: &str = include_str!("diag.ps1");
pub const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";

pub fn run_script(progress: &Progress) -> Result<Raw, String> {
    use std::io::{BufRead, BufReader};
    use std::sync::atomic::Ordering;
    let path = std::env::temp_dir().join(format!("nexo_diag_{}.ps1", std::process::id()));
    // PowerShell 5.1 necesita BOM para leer bien los acentos del script.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(SCRIPT.as_bytes());
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    progress.store(40, Ordering::Relaxed);
    let child = sys::hidden_cmd("powershell")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_file(&path);
            return Err(e.to_string());
        }
    };
    let mut text = String::new();
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).split(b'\n').map_while(Result::ok) {
            let line = String::from_utf8_lossy(&line).to_string();
            if let Some(rest) = line.trim().strip_prefix("@@PROGRESO ") {
                // "3/8 discos" → 3 de 8 partes listas (el 90 % de la barra; el resto es el análisis).
                let frac = rest.split_whitespace().next().and_then(|f| f.split_once('/')).and_then(|(a, b)| Some((a.parse::<f32>().ok()?, b.parse::<f32>().ok()?)));
                if let Some((a, b)) = frac {
                    progress.store((60.0 + 860.0 * a / b.max(1.0)) as u32, Ordering::Relaxed);
                }
            } else {
                text.push_str(&line);
                text.push('\n');
            }
        }
    }
    let _ = child.wait();
    let _ = std::fs::remove_file(&path);
    let (Some(a), Some(b)) = (text.find('{'), text.rfind('}')) else {
        return Err("El diagnóstico no devolvió datos.".into());
    };
    serde_json::from_str(&text[a..=b]).map_err(|e| format!("Datos de diagnóstico no válidos: {}", e))
}

pub fn active_power_plan() -> (String, String) {
    let Ok(out) = sys::hidden_cmd("powercfg").arg("/getactivescheme").output() else {
        return (String::new(), String::new());
    };
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let guid = text
        .split_whitespace()
        .find(|t| t.len() == 36 && t.matches('-').count() == 4)
        .unwrap_or("")
        .to_lowercase();
    let name = match (text.rfind('('), text.rfind(')')) {
        (Some(a), Some(b)) if b > a => text[a + 1..b].trim().to_string(),
        _ => String::new(),
    };
    (guid, name)
}

pub fn set_power_plan(guid: &str) -> bool {
    sys::hidden_cmd("powercfg").args(["/setactive", guid]).status().map(|s| s.success()).unwrap_or(false)
}

/// Activa o desactiva un programa de inicio igual que el Administrador de tareas
/// (StartupApproved). No borra nada: se puede volver a activar en cualquier momento.
pub fn set_startup_enabled(item: &StartupItem, on: bool) -> Result<(), String> {
    let hive = if item.machine_wide() { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER };
    let key = format!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\{}", item.sub);
    let mut data = [0u8; 12];
    data[0] = if on { 2 } else { 3 };
    if !on {
        let ft = (fmt::now() + 11_644_473_600) * 10_000_000;
        data[4..12].copy_from_slice(&ft.to_le_bytes());
    }
    let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let r = unsafe { RegSetKeyValueW(hive, w(&key).as_ptr(), w(&item.name).as_ptr(), REG_BINARY, data.as_ptr() as *const _, 12) };
    if r == 0 {
        Ok(())
    } else if item.machine_wide() {
        Err("Este programa es de todo el equipo: abre NEXO como administrador para cambiarlo.".into())
    } else {
        Err(format!("No se pudo cambiar (error {}).", r))
    }
}

/// Análisis rápido de Microsoft Defender. Devuelve el código de salida (0 = limpio).
pub fn quick_scan() -> Result<i32, String> {
    let pf = std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap_or_else(|| "C:\\Program Files".into());
    let exe = pf.join("Windows Defender").join("MpCmdRun.exe");
    sys::hidden_cmd(&exe.to_string_lossy())
        .args(["-Scan", "-ScanType", "1"])
        .status()
        .map(|s| s.code().unwrap_or(-1))
        .map_err(|e| e.to_string())
}

/// Genera el informe oficial de batería de Windows y lo abre en el navegador (local).
pub fn battery_report() {
    let out = std::env::temp_dir().join("NEXO_informe_bateria.html");
    let ok = sys::hidden_cmd("powercfg").arg("/batteryreport").arg("/output").arg(&out).status().map(|s| s.success()).unwrap_or(false);
    if ok {
        let _ = std::process::Command::new("explorer").arg(&out).spawn();
    }
}

/// Abre un ajuste de Windows (ms-settings:, windowsdefender:, taskschd.msc...).
pub fn open(target: &str) {
    let _ = sys::hidden_cmd("cmd").args(["/c", "start", "", target]).spawn();
}


// ── Textos y accesos directos propios de Windows (los usa `analyze`).
pub const AV_MISSING_SEVERITY: u8 = 1;
pub fn quick_scan_available() -> bool {
    true
}
pub const CHECK_SIGNATURES: bool = true;

pub fn txt(k: &str) -> &'static str {
    match k {
        "os" => "Windows",
        "av" => "Microsoft Defender",
        "uac_name" => "Control de cuentas (UAC)",
        "uac_on" => "Activado: te pregunta antes de cambios importantes",
        "remote_name" => "Escritorio remoto",
        "security_app" => "Seguridad de Windows",
        "updater" => "Windows Update",
        "av_missing_title" => "No se pudo leer Microsoft Defender",
        "av_missing_detail" => "Si usas otro antivirus es normal. Si no, revisa Seguridad de Windows.",
        "uac_off" => "Control de cuentas (UAC) desactivado",
        "uac_detail" => "Cualquier programa puede hacer cambios de administrador sin preguntarte.",
        "remote_on" => "Escritorio remoto activado",
        "encryption" => "BitLocker / cifrado de dispositivo",
        "reboot_note" => " (Apagar con inicio rápido no cuenta.)",
        "lenovo_detail" => "En Lenovo Vantage › Energía: mantiene la carga cerca del 80 % cuando trabajas enchufado y alarga la vida de la batería.",
        _ => "",
    }
}

pub fn setting(k: &str) -> Option<&'static str> {
    Some(match k {
        "antivirus" => "windowsdefender:",
        "realtime" => "windowsdefender://threat",
        "firewall" => "windowsdefender://network",
        "updates" => "ms-settings:windowsupdate",
        "remote" => "ms-settings:remotedesktop",
        "proxy" => "ms-settings:network-proxy",
        "encryption" => "ms-settings:deviceencryption",
        "tasks" => "taskschd.msc",
        "lenovo" => "shell:AppsFolder\\E046963F.LenovoCompanion_k1h2ywk1493x8!App",
        _ => return None,
    })
}
