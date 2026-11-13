//! Limpieza automática programada en Linux (temporizador de systemd del usuario, o cron
//! si systemd no está disponible) y macOS (LaunchAgent con calendario de launchd).
//! NEXO no queda residente: el sistema lo lanza con `--auto`, limpia y se cierra.

use std::path::PathBuf;
use std::process::Command;

use crate::config::{Config, Frequency};
use crate::install;

#[allow(dead_code)]
pub const TASK: &str = "NEXO-LimpiezaAutomatica";

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

fn ok(c: &mut Command) -> bool {
    c.output().map(|o| o.status.success()).unwrap_or(false)
}

// ───────────────────────── macOS ─────────────────────────

#[cfg(target_os = "macos")]
fn plist_path() -> PathBuf {
    home().join("Library/LaunchAgents/com.nexo.limpieza.plist")
}

#[cfg(target_os = "macos")]
pub fn enable(cfg: &Config, _admin: bool) -> Result<(), String> {
    let exe = install::install()?;
    // Semanal: domingo (Weekday 0) · Mensual: día 1 · Diario: solo la hora.
    let when = match cfg.frequency {
        Frequency::Daily => String::new(),
        Frequency::Weekly => "<key>Weekday</key><integer>0</integer>".into(),
        Frequency::Monthly => "<key>Day</key><integer>1</integer>".into(),
    };
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>com.nexo.limpieza</string>
<key>ProgramArguments</key><array><string>{}</string><string>--auto</string></array>
<key>StartCalendarInterval</key><dict><key>Hour</key><integer>{}</integer><key>Minute</key><integer>0</integer>{}</dict>
<key>ProcessType</key><string>Background</string>
<key>LowPriorityIO</key><true/>
</dict></plist>
"#,
        exe.display(),
        cfg.hour,
        when
    );
    let p = plist_path();
    let _ = std::fs::create_dir_all(p.parent().unwrap_or(&p));
    let _ = Command::new("launchctl").arg("unload").arg(&p).output();
    std::fs::write(&p, plist).map_err(|e| e.to_string())?;
    if ok(Command::new("launchctl").arg("load").arg("-w").arg(&p)) {
        Ok(())
    } else {
        Err("launchctl no aceptó la tarea.".into())
    }
}

#[cfg(target_os = "macos")]
pub fn disable() -> Result<(), String> {
    let p = plist_path();
    if !p.exists() {
        return Ok(());
    }
    let _ = Command::new("launchctl").arg("unload").arg("-w").arg(&p).output();
    std::fs::remove_file(&p).map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
pub fn is_registered() -> bool {
    plist_path().exists()
}

// ───────────────────────── Linux ─────────────────────────

#[cfg(not(target_os = "macos"))]
fn unit_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("systemd/user")
}

#[cfg(not(target_os = "macos"))]
fn has_systemd() -> bool {
    ok(Command::new("systemctl").args(["--user", "show-environment"]))
}

#[cfg(not(target_os = "macos"))]
const CRON_MARK: &str = "# NEXO-LimpiezaAutomatica";

#[cfg(not(target_os = "macos"))]
fn crontab() -> String {
    Command::new("crontab").arg("-l").output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
}

#[cfg(not(target_os = "macos"))]
fn set_crontab(text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = Command::new("crontab").arg("-").stdin(std::process::Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    child.stdin.take().ok_or("sin entrada")?.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    if child.wait().map(|s| s.success()).unwrap_or(false) {
        Ok(())
    } else {
        Err("crontab no aceptó la tarea.".into())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn enable(cfg: &Config, _admin: bool) -> Result<(), String> {
    let exe = install::install()?;
    let h = cfg.hour;
    if has_systemd() {
        let dir = unit_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let when = match cfg.frequency {
            Frequency::Daily => format!("*-*-* {:02}:00:00", h),
            Frequency::Weekly => format!("Sun *-*-* {:02}:00:00", h),
            Frequency::Monthly => format!("*-*-01 {:02}:00:00", h),
        };
        let service = format!(
            "[Unit]\nDescription=NEXO: limpieza automática (nunca borra videos ni documentos)\n\n[Service]\nType=oneshot\nExecStart=\"{}\" --auto\nNice=10\nIOSchedulingClass=idle\n",
            exe.display()
        );
        // Persistent: si el equipo estaba apagado a esa hora, limpia al encenderse.
        let timer = format!("[Unit]\nDescription=NEXO: horario de la limpieza automática\n\n[Timer]\nOnCalendar={}\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n", when);
        std::fs::write(dir.join("nexo-limpieza.service"), service).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("nexo-limpieza.timer"), timer).map_err(|e| e.to_string())?;
        let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).output();
        if ok(Command::new("systemctl").args(["--user", "enable", "--now", "nexo-limpieza.timer"])) {
            return Ok(());
        }
        return Err("systemd no aceptó el temporizador.".into());
    }
    let dow = match cfg.frequency {
        Frequency::Daily => ("*", "*"),
        Frequency::Weekly => ("*", "0"),
        Frequency::Monthly => ("1", "*"),
    };
    let mut lines: Vec<String> = crontab().lines().filter(|l| !l.contains(CRON_MARK)).map(String::from).collect();
    lines.push(format!("0 {} {} * {} \"{}\" --auto {}", h, dow.0, dow.1, exe.display(), CRON_MARK));
    set_crontab(&(lines.join("\n") + "\n"))
}

#[cfg(not(target_os = "macos"))]
pub fn disable() -> Result<(), String> {
    let dir = unit_dir();
    if dir.join("nexo-limpieza.timer").exists() {
        let _ = Command::new("systemctl").args(["--user", "disable", "--now", "nexo-limpieza.timer"]).output();
        let _ = std::fs::remove_file(dir.join("nexo-limpieza.timer"));
        let _ = std::fs::remove_file(dir.join("nexo-limpieza.service"));
        let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).output();
    }
    let cron = crontab();
    if cron.contains(CRON_MARK) {
        let rest: Vec<&str> = cron.lines().filter(|l| !l.contains(CRON_MARK)).collect();
        set_crontab(&(rest.join("\n") + "\n"))?;
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn is_registered() -> bool {
    unit_dir().join("nexo-limpieza.timer").exists() || crontab().contains(CRON_MARK)
}
