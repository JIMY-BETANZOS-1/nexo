use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

// Paleta "cabina mecha": azul holográfico sobre acero oscuro, naranja de advertencia.
pub const BG: Color32 = Color32::from_rgb(3, 7, 13);
pub const PANEL: Color32 = Color32::from_rgb(7, 15, 26);
pub const PANEL_HI: Color32 = Color32::from_rgb(13, 27, 44);
pub const CYAN: Color32 = Color32::from_rgb(56, 200, 255);
pub const ORANGE: Color32 = Color32::from_rgb(255, 138, 30);
pub const MAGENTA: Color32 = Color32::from_rgb(255, 72, 196);
pub const GREEN: Color32 = Color32::from_rgb(60, 255, 180);
pub const AMBER: Color32 = Color32::from_rgb(255, 196, 48);
pub const RED: Color32 = Color32::from_rgb(255, 48, 72);
pub const TEXT: Color32 = Color32::from_rgb(212, 236, 255);
pub const MUTED: Color32 = Color32::from_rgb(108, 136, 166);

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

/// Usa fuentes que ya trae Windows (Bahnschrift / Cascadia): nada que descargar ni empaquetar.
pub fn setup(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windows_fonts = std::env::var_os("WINDIR")
        .map(|w| std::path::PathBuf::from(w).join("Fonts"))
        .unwrap_or_else(|| "C:\\Windows\\Fonts".into());

    let mut display_list = Vec::new();
    if let Ok(b) = std::fs::read(windows_fonts.join("bahnschrift.ttf")) {
        fonts.font_data.insert("bahn".into(), FontData::from_owned(b));
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "bahn".into());
        display_list.push("bahn".to_string());
    }
    if let Ok(b) = std::fs::read(windows_fonts.join("CascadiaMono.ttf")) {
        fonts.font_data.insert("cascadia".into(), FontData::from_owned(b));
        fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "cascadia".into());
    }
    display_list.extend(fonts.families[&FontFamily::Proportional].iter().filter(|&n| n != "bahn").cloned());
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
    v.extreme_bg_color = Color32::from_rgb(4, 7, 14);
    v.faint_bg_color = PANEL_HI;
    v.hyperlink_color = CYAN;
    v.selection.bg_fill = alpha(CYAN, 70);
    v.selection.stroke = egui::Stroke::new(1.0_f32, CYAN);
    v.window_rounding = 2.0.into();
    v.window_stroke = egui::Stroke::new(1.0_f32, alpha(CYAN, 60));
    for w in [&mut v.widgets.inactive, &mut v.widgets.noninteractive] {
        w.bg_fill = PANEL_HI;
        w.weak_bg_fill = PANEL_HI;
        w.rounding = 2.0.into();
    }
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, alpha(CYAN, 50));
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.5_f32, TEXT);
    v.widgets.hovered.bg_fill = alpha(CYAN, 40);
    v.widgets.hovered.weak_bg_fill = alpha(CYAN, 40);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, CYAN);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.5_f32, CYAN);
    v.widgets.hovered.rounding = 2.0.into();
    v.widgets.active.bg_fill = alpha(CYAN, 80);
    v.widgets.active.weak_bg_fill = alpha(CYAN, 80);
    v.widgets.active.fg_stroke = egui::Stroke::new(2.0_f32, Color32::WHITE);
    v.widgets.active.rounding = 2.0.into();
    v.widgets.open.bg_fill = PANEL_HI;
    v.widgets.open.weak_bg_fill = PANEL_HI;
    ctx.set_style(style);
}
