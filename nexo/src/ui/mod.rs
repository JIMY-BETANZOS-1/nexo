mod app;
mod theme;
mod thumbcache;
mod widgets;

use eframe::egui;

pub fn run(start_page: Option<String>) {
    // La ventana de NEXO trabaja siempre como administrador (para limpiar también lo de
    // Windows). Si el usuario rechaza el aviso, se abre igual con permisos normales.
    // NEXO_SIN_ELEVAR=1 lo evita (solo para pruebas automáticas).
    // En Linux y macOS NEXO trabaja como usuario normal: lo que requiere permisos del sistema
    // se hace con un comando que pide la contraseña en ese momento (pkexec / osascript).
    if cfg!(windows) && !crate::sys::is_admin() && std::env::var_os("NEXO_SIN_ELEVAR").is_none() {
        let params = start_page.as_deref().map(|p| format!("--pagina {}", p)).unwrap_or_default();
        if crate::sys::relaunch_as_admin_with(&params) {
            return;
        }
    }
    if !crate::sys::claim_single_instance() {
        return;
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NEXO")
            .with_inner_size([1200.0, 740.0])
            .with_min_inner_size([1000.0, 640.0])
            .with_decorations(false)
            .with_icon(egui::IconData { rgba: crate::icon::rgba(64), width: 64, height: 64 }),
        centered: true,
        ..Default::default()
    };
    let _ = eframe::run_native("NEXO", options, Box::new(|cc| Ok(Box::new(app::NexoApp::new(cc, start_page)))));
}
