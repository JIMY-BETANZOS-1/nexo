mod app;
mod theme;
mod thumbcache;
mod widgets;

use eframe::egui;

pub fn jb_run(start_page: Option<String>) {
    if cfg!(windows) && !crate::sys::jb_is_admin() && std::env::var_os("NEXO_SIN_ELEVAR").is_none() {
        let params = start_page.as_deref().map(|p| format!("--pagina {}", p)).unwrap_or_default();
        if crate::sys::jb_relaunch_as_admin_with(&params) {
            return;
        }
    }
    if std::env::var_os("NEXO_CAPTURA").is_none() && !crate::sys::jb_claim_single_instance() {
        return;
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NEXO")
            .with_inner_size([1200.0, 740.0])
            .with_min_inner_size([1000.0, 640.0])
            .with_decorations(false)
            .with_icon(egui::IconData { rgba: crate::icon::jb_rgba(64), width: 64, height: 64 }),
        centered: true,
        ..Default::default()
    };
    let _ = eframe::run_native("NEXO", options, Box::new(|cc| Ok(Box::new(app::JbNexoApp::jb_new(cc, start_page)))));
}
