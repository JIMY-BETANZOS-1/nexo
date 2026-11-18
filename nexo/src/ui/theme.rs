// ════════════════════════════════════════════════════════════════════════
//  NEXO · nexo/src/ui/theme.rs
//  Autor: Chatta Betanzos, Jimy Alexander  ·  jimy-betanzos
// ════════════════════════════════════════════════════════════════════════

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

// jimy-betanzos: un solo color de acento y grises: la interfaz no distrae y los avisos
// (ámbar y rojo) resaltan solo cuando de verdad hay algo que revisar.
// Paleta sobria de un solo color: grises neutros y azul acero como único acento.
// El ámbar y el rojo (apagados) se reservan para avisos reales: disco lleno, archivo sospechoso.
pub const BG: Color32 = Color32::from_rgb(22, 25, 29);
pub const PANEL: Color32 = Color32::from_rgb(30, 34, 39);
pub const PANEL_HI: Color32 = Color32::from_rgb(44, 50, 56);
pub const ACCENT: Color32 = Color32::from_rgb(122, 151, 180);
pub const WARN: Color32 = Color32::from_rgb(192, 164, 108);
pub const DANGER: Color32 = Color32::from_rgb(196, 112, 112);
pub const TEXT: Color32 = Color32::from_rgb(214, 217, 221);
pub const MUTED: Color32 = Color32::from_rgb(138, 146, 155);
/// Texto sobre botones rellenos del color de acento.
pub const LIGHT: Color32 = Color32::from_rgb(245, 247, 249);

/// Transparencia mezclada en espacio gamma: los brillos suaves se quedan suaves.
pub fn alpha(c: Color32, a: u8) -> Color32 {
    let m = |v: u8| ((v as u16 * a as u16) / 255) as u8;
    Color32::from_rgba_premultiplied(m(c.r()), m(c.g()), m(c.b()), a)
}

pub fn display() -> FontFamily {
    FontFamily::Name("display".into())
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, display())
}

/// Usa fuentes que ya trae el sistema: nada que descargar ni empaquetar.
/// Windows: Segoe UI y Cascadia · macOS: Helvetica Neue y Menlo · Linux: DejaVu.
pub fn setup(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windows_fonts = std::env::var_os("WINDIR")
        .map(|w| std::path::PathBuf::from(w).join("Fonts"))
        .unwrap_or_else(|| "C:\\Windows\\Fonts".into());
    let display_candidates = [
        windows_fonts.join("segoeuisb.ttf"),
        "/System/Library/Fonts/HelveticaNeue.ttc".into(),
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf".into(),
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf".into(),
    ];
    let body_candidates = [windows_fonts.join("segoeui.ttf")];
    let mono_candidates = [
        windows_fonts.join("CascadiaMono.ttf"),
        "/System/Library/Fonts/Menlo.ttc".into(),
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf".into(),
        "/usr/share/fonts/dejavu-sans-mono-fonts/DejaVuSansMono.ttf".into(),
    ];
    let first = |c: &[std::path::PathBuf]| c.iter().find_map(|p| std::fs::read(p).ok());

    let mut display_list = Vec::new();
    if let Some(b) = first(&display_candidates) {
        fonts.font_data.insert("titulo".into(), FontData::from_owned(b));
        display_list.push("titulo".to_string());
    }
    if let Some(b) = first(&body_candidates) {
        fonts.font_data.insert("segoe".into(), FontData::from_owned(b));
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "segoe".into());
    }
    if let Some(b) = first(&mono_candidates) {
        fonts.font_data.insert("cascadia".into(), FontData::from_owned(b));
        fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "cascadia".into());
    }
    display_list.extend(fonts.families[&FontFamily::Proportional].iter().filter(|&n| n != "titulo").cloned());
    fonts.families.insert(display(), display_list);
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(24.0)),
        (TextStyle::Body, FontId::proportional(15.0)),
        (TextStyle::Button, FontId::proportional(15.0)),
        (TextStyle::Small, FontId::proportional(12.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size.y = 26.0;
    style.spacing.scroll = egui::style::ScrollStyle::thin();

    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(18, 21, 24);
    v.faint_bg_color = PANEL_HI;
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = alpha(ACCENT, 70);
    v.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT);
    v.window_rounding = 4.0.into();
    v.window_stroke = egui::Stroke::new(1.0_f32, alpha(ACCENT, 60));
    for w in [&mut v.widgets.inactive, &mut v.widgets.noninteractive] {
        w.bg_fill = PANEL_HI;
        w.weak_bg_fill = PANEL_HI;
        w.rounding = 4.0.into();
    }
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, alpha(ACCENT, 50));
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.5_f32, TEXT);
    v.widgets.hovered.bg_fill = alpha(ACCENT, 40);
    v.widgets.hovered.weak_bg_fill = alpha(ACCENT, 40);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.5_f32, ACCENT);
    v.widgets.hovered.rounding = 4.0.into();
    v.widgets.active.bg_fill = alpha(ACCENT, 80);
    v.widgets.active.weak_bg_fill = alpha(ACCENT, 80);
    v.widgets.active.fg_stroke = egui::Stroke::new(2.0_f32, Color32::WHITE);
    v.widgets.active.rounding = 4.0.into();
    v.widgets.open.bg_fill = PANEL_HI;
    v.widgets.open.weak_bg_fill = PANEL_HI;
    ctx.set_style(style);
}
