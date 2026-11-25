use crate::config::{JbConfig, JbFrequency};
use crate::{install, sys};

pub const TASK: &str = "NEXO\\LimpiezaAutomatica";

fn jb_xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn jb_task_xml(cfg: &JbConfig, exe: &str, admin: bool) -> String {
    let start = format!("2025-01-05T{:02}:00:00", cfg.hour);
    let schedule = match cfg.frequency {
        JbFrequency::Daily => "<ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay>".to_string(),
        JbFrequency::Weekly => {
            "<ScheduleByWeek><WeeksInterval>1</WeeksInterval><DaysOfWeek><Sunday /></DaysOfWeek></ScheduleByWeek>".to_string()
        }
        JbFrequency::Monthly => "<ScheduleByMonth><DaysOfMonth><Day>1</Day></DaysOfMonth><Months><January /><February /><March /><April /><May /><June /><July /><August /><September /><October /><November /><December /></Months></ScheduleByMonth>".to_string(),
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
        exe = jb_xml_escape(exe)
    )
}

pub fn jb_enable(cfg: &JbConfig, admin: bool) -> Result<(), String> {
    let exe = install::jb_install()?;
    let xml = jb_task_xml(cfg, &exe.display().to_string(), admin);
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let file = std::env::temp_dir().join("nexo_tarea.xml");
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    let out = sys::jb_hidden_cmd("schtasks")
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

pub fn jb_disable() -> Result<(), String> {
    if !jb_is_registered() {
        return Ok(());
    }
    let out = sys::jb_hidden_cmd("schtasks")
        .args(["/Delete", "/TN", TASK, "/F"])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn jb_is_registered() -> bool {
    sys::jb_hidden_cmd("schtasks")
        .args(["/Query", "/TN", TASK])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
