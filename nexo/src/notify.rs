// Proyecto: NEXO
// Archivo: notify.rs
// Autor: Chatta Betanzos, Jimy Alexander (jimy-betanzos)

//! Notificaciones de Windows con el nombre y el icono de NEXO.

// jimy-betanzos - dependencia: notificaciones de Windows
use tauri_winrt_notification::Toast;

// jimy-betanzos - dependencia: módulo propio install
use crate::install;

/// jimy-betanzos: Muestra una notificación de Windows.
pub fn jb_toast(title: &str, line1: &str, line2: &str) {
    let id = if install::jb_notifications_registered() { install::AUMID } else { Toast::POWERSHELL_APP_ID };
    let mut t = Toast::new(id).title(title).text1(line1);
    if !line2.is_empty() {
        t = t.text2(line2);
    }
    let _ = t.show();
}
