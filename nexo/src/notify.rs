use tauri_winrt_notification::Toast;

use crate::install;

pub fn jb_toast(title: &str, line1: &str, line2: &str) {
    let id = if install::jb_notifications_registered() { install::AUMID } else { Toast::POWERSHELL_APP_ID };
    let mut t = Toast::new(id).title(title).text1(line1);
    if !line2.is_empty() {
        t = t.text2(line2);
    }
    let _ = t.show();
}
