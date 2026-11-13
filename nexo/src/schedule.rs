//! Tarea programada de Windows para el modo automático.
//! La app no queda residente: Windows la lanza, limpia y se cierra (0 MB en reposo).

use crate::config::{Config, Frequency};
use crate::{install, sys};

pub const TASK: &str = "NEXO\\LimpiezaAutomatica";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn task_xml(cfg: &Config, exe: &str, admin: bool) -> String {
    let start = format!("2025-01-05T{:02}:00:00", cfg.hour);
    let schedule = match cfg.frequency {
        Frequency::Daily => "<ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay>".to_string(),
        Frequency::Weekly => {
            "<ScheduleByWeek><WeeksInterval>1</WeeksInterval><DaysOfWeek><Sunday /></DaysOfWeek></ScheduleByWeek>".to_string()
        }
        Frequency::Monthly => "<ScheduleByMonth><DaysOfMonth><Day>1</Day></DaysOfMonth><Months><January /><February /><March /><April /><May /><June /><July /><August /><September /><October /><November /><December /></Months></ScheduleByMonth>".to_string(),
    };
    let level = if admin { "HighestAvailable" } else { "LeastPrivilege" };
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>NEXO: limpieza automática de archivos temporales y caché. Nunca borra videos ni documentos.</Description></RegistrationInfo>
  <Triggers>
    <CalendarTrigger><StartBoundary>{start}</StartBoundary><Enabled>true</Enabled>{schedule}</CalendarTrigger>
  </Triggers>
  <Principals><Principal id="Author"><LogonType>InteractiveToken</LogonType><RunLevel>{level}</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings><StopOnIdleEnd>false</StopOnIdleEnd><RestartOnIdle>false</RestartOnIdle></IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT3H</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command><Arguments>--auto</Arguments></Exec></Actions>
</Task>"#,
        start = start,
        schedule = schedule,
        level = level,
        exe = xml_escape(exe)
    )
}

pub fn enable(cfg: &Config, admin: bool) -> Result<(), String> {
    // Copia estable del ejecutable: la tarea no depende de dónde se abrió NEXO.
    let exe = install::install()?;
    let xml = task_xml(cfg, &exe.display().to_string(), admin);
    // schtasks exige UTF-16 LE con BOM.
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let file = std::env::temp_dir().join("nexo_tarea.xml");
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    let out = sys::hidden_cmd("schtasks")
        .args(["/Create", "/TN", TASK, "/XML"])
        .arg(&file)
        .arg("/F")
        .output()
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&file);
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn disable() -> Result<(), String> {
    if !is_registered() {
        return Ok(());
    }
    let out = sys::hidden_cmd("schtasks")
        .args(["/Delete", "/TN", TASK, "/F"])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn is_registered() -> bool {
    sys::hidden_cmd("schtasks")
        .args(["/Query", "/TN", TASK])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
