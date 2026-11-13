//! Notificaciones en Linux (`notify-send`, estándar freedesktop) y macOS (Centro de
//! notificaciones mediante `osascript`).

pub fn toast(title: &str, line1: &str, line2: &str) {
    let body = if line2.is_empty() { line1.to_string() } else { format!("{}\n{}", line1, line2) };
    #[cfg(target_os = "macos")]
    {
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!("display notification \"{}\" with title \"NEXO\" subtitle \"{}\"", esc(&body), esc(title));
        let _ = std::process::Command::new("osascript").args(["-e", &script]).status();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let icon = crate::install::install_dir().join("nexo.png");
        let mut c = std::process::Command::new("notify-send");
        c.args(["-a", "NEXO"]);
        if icon.exists() {
            c.arg("-i").arg(icon);
        }
        let _ = c.arg(title).arg(body).status();
    }
}
