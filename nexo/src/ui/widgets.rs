// ════════════════════════════════════════════════════════════════════════
//  NEXO · nexo/src/ui/widgets.rs
//  Autor: Chatta Betanzos, Jimy Alexander  ·  jimy-betanzos
// ════════════════════════════════════════════════════════════════════════

//! Componentes visuales de NEXO: tarjetas planas con borde fino, anillos de progreso,
//! barras, botones y etiquetas. Todo es vectorial (sin imágenes): el ejecutable sigue
//! siendo pequeño. Estilo sobrio de un solo color de acento (ver theme.rs).

use std::f32::consts::{FRAC_PI_2, TAU};

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CursorIcon, Frame, InnerResponse, Margin, Painter, Pos2, Rect,
    Response, RichText, Sense, Shape, Stroke, Ui, Vec2,
};

use super::theme::*;

// ───────────────────────────── geometría ─────────────────────────────

const ROUND: f32 = 6.0;

/// Tarjeta plana: fondo, tinte opcional del color de estado y borde de 1 px.
fn card_shapes(r: Rect, fill: Color32, tint: Option<Color32>, stroke: Stroke) -> Vec<Shape> {
    let mut v = vec![Shape::rect_filled(r, ROUND, fill)];
    if let Some(t) = tint {
        v.push(Shape::rect_filled(r, ROUND, t));
    }
    v.push(Shape::rect_stroke(r, ROUND, stroke));
    v
}

// ───────────────────────────── paneles ─────────────────────────────

/// Panel de cabina. Mismas opciones que un marco normal (relleno, borde, margen).
#[derive(Clone, Copy)]
pub struct Card {
    fill: Color32,
    tint: Option<Color32>,
    stroke: Stroke,
    margin: Margin,
}

impl Card {
    pub fn fill(mut self, c: Color32) -> Self {
        self.fill = c;
        self
    }
    pub fn stroke(mut self, s: Stroke) -> Self {
        self.stroke = s;
        self
    }
    pub fn inner_margin(mut self, m: Margin) -> Self {
        self.margin = m;
        self
    }
    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        // Se reserva el fondo antes del contenido y se dibuja al conocer su tamaño.
        let bg = ui.painter().add(Shape::Noop);
        // Las columnas de egui justifican el texto: en pantallas angostas una línea partida
        // («95,7 GB / libres») se estiraba letra por letra. Dentro de la tarjeta no se justifica.
        let r = Frame::none().inner_margin(self.margin).show(ui, |ui| {
            let layout = ui.layout().with_cross_justify(false);
            ui.with_layout(layout, add).inner
        });
        ui.painter().set(bg, Shape::Vec(card_shapes(r.response.rect, self.fill, self.tint, self.stroke)));
        r
    }
}

pub fn card() -> Card {
    Card { fill: PANEL, tint: None, stroke: Stroke::new(1.0_f32, PANEL_HI), margin: Margin::same(18.0) }
}

/// Tarjeta con un leve tinte del color indicado (acento o aviso).
pub fn tinted_card(c: Color32) -> Card {
    Card { fill: PANEL, tint: Some(alpha(c, 14)), stroke: Stroke::new(1.0_f32, alpha(c, 70)), margin: Margin::same(18.0) }
}

// ───────────────────────────── fondo ─────────────────────────────

pub fn background(p: &Painter, rect: Rect) {
    p.rect_filled(rect, 0.0, BG);
}

pub fn arc_points(c: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let n = ((a1 - a0).abs() * r / 3.0).max(8.0) as usize;
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}

/// Indicador circular de progreso: pista gris y arco del color indicado.
pub fn ring(p: &Painter, c: Pos2, r: f32, frac: f32, color: Color32, w: f32) {
    p.circle_stroke(c, r, Stroke::new(w, PANEL_HI));
    let f = frac.clamp(0.0, 1.0);
    if f > 0.0 {
        p.add(Shape::line(arc_points(c, r, -FRAC_PI_2, -FRAC_PI_2 + f * TAU), Stroke::new(w, color)));
    }
}

/// Indicador de actividad mientras se escanea: un arco que gira sobre una pista gris.
pub fn scanner(p: &Painter, c: Pos2, r: f32, t: f32, color: Color32) {
    p.circle_stroke(c, r, Stroke::new(4.0_f32, PANEL_HI));
    let a0 = t * 3.0;
    p.add(Shape::line(arc_points(c, r, a0, a0 + 1.6), Stroke::new(4.0_f32, color)));
}

/// Logotipo: hexágono con un punto central, en el color de acento.
pub fn logo(p: &Painter, c: Pos2, r: f32) {
    let pts: Vec<Pos2> = (0..6)
        .map(|i| {
            let a = i as f32 * TAU / 6.0 - FRAC_PI_2;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    p.add(Shape::closed_line(pts, Stroke::new(2.0_f32, ACCENT)));
    p.circle_filled(c, r * 0.28, ACCENT);
}

// ───────────────────────────── controles ─────────────────────────────

/// Botón principal: relleno del color indicado y texto claro.
pub fn button(ui: &mut Ui, text: &str, color: Color32, size: Vec2, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let h = ui.ctx().animate_bool(resp.id, enabled && resp.hovered());
    let p = ui.painter();
    if enabled {
        p.rect_filled(rect, ROUND, alpha(color, (150.0 + 60.0 * h) as u8));
    } else {
        p.rect_stroke(rect, ROUND, Stroke::new(1.0_f32, PANEL_HI));
    }
    let txt = if enabled { LIGHT } else { MUTED };
    p.text(rect.center(), Align2::CENTER_CENTER, text, font(size.y.min(52.0) * 0.32 + 3.0), txt);
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Botón de texto pequeño (enlaces de acción).
pub fn link(ui: &mut Ui, text: &str, color: Color32) -> Response {
    ui.add(egui::Label::new(RichText::new(text).color(color).size(13.0)).sense(Sense::click()))
        .on_hover_cursor(CursorIcon::PointingHand)
}

pub fn chip(ui: &mut Ui, text: &str, selected: bool) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), font(13.5), TEXT);
    let size = galley.size() + vec2(28.0, 12.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let h = ui.ctx().animate_bool(resp.id, resp.hovered());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, ROUND, alpha(ACCENT, 170));
    } else {
        p.rect_filled(rect, ROUND, alpha(PANEL_HI, (120.0 + 120.0 * h) as u8));
    }
    p.text(rect.center(), Align2::CENTER_CENTER, text, font(13.5), if selected { LIGHT } else { TEXT });
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Etiqueta de estado pequeña.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), font(11.0), color);
    let size = galley.size() + vec2(14.0, 6.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, alpha(color, 26));
    p.galley(rect.center() - galley.size() / 2.0, galley, color);
}

pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(44.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool(resp.id, *on);
    let p = ui.painter();
    p.rect_filled(rect, 11.0, PANEL_HI.lerp_to_gamma(ACCENT, t));
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    p.circle_filled(pos2(x, rect.center().y), 8.0, TEXT);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Barra de nivel continua.
pub fn bar(ui: &mut Ui, frac: f32, color: Color32, height: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, height.max(5.0)), Sense::hover());
    let p = ui.painter();
    let rd = rect.height() / 2.0;
    p.rect_filled(rect, rd, PANEL_HI);
    let f = frac.clamp(0.0, 1.0);
    if f > 0.0 {
        p.rect_filled(Rect::from_min_size(rect.min, vec2((rect.width() * f).max(rect.height()), rect.height())), rd, color);
    }
}

/// Barra de progreso. Con `None` (avance desconocido) un tramo recorre la barra.
pub fn progress_bar(ui: &mut Ui, frac: Option<f32>, color: Color32, t: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 6.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 3.0, PANEL_HI);
    let (x0, x1) = match frac {
        Some(f) => (0.0, f.clamp(0.0, 1.0)),
        None => {
            let head = (t * 0.6).fract() * 1.3;
            ((head - 0.3).max(0.0), head.min(1.0))
        }
    };
    if x1 > x0 {
        let r = Rect::from_min_max(pos2(rect.left() + rect.width() * x0, rect.top()), pos2(rect.left() + rect.width() * x1, rect.bottom()));
        p.rect_filled(r, 3.0, color);
    }
}

pub fn level_color(frac: f32) -> Color32 {
    if frac >= 0.9 {
        DANGER
    } else if frac >= 0.75 {
        WARN
    } else {
        ACCENT
    }
}

pub fn stat_card(ui: &mut Ui, icon: &str, title: &str, value: &str, sub: &str, color: Color32, frac: Option<f32>) {
    tinted_card(color).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(112.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon).size(15.0).color(color));
            ui.label(RichText::new(title).size(12.5).color(MUTED));
        });
        ui.label(RichText::new(value).font(font(26.0)).color(TEXT));
        ui.label(RichText::new(sub).size(12.0).color(MUTED));
        if let Some(f) = frac {
            ui.add_space(2.0);
            bar(ui, f, color, 6.0);
        }
    });
}

pub fn nav_item(ui: &mut Ui, icon: &str, label: &str, selected: bool, tag: Option<&str>) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let h = ui.ctx().animate_bool(resp.id, resp.hovered());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, ROUND, PANEL_HI);
        p.rect_filled(Rect::from_min_size(rect.left_top() + vec2(0.0, 10.0), vec2(3.0, rect.height() - 20.0)), 1.5, ACCENT);
    } else if h > 0.0 {
        p.rect_filled(rect, ROUND, alpha(PANEL_HI, (h * 140.0) as u8));
    }
    let col = if selected { TEXT } else if tag.is_some() { alpha(TEXT, 130) } else { MUTED };
    let icol = if selected { ACCENT } else { col };
    p.text(rect.left_center() + vec2(20.0, 0.0), Align2::CENTER_CENTER, icon, egui::FontId::proportional(16.0), icol);
    p.text(rect.left_center() + vec2(42.0, 0.0), Align2::LEFT_CENTER, label, egui::FontId::proportional(14.5), col);
    if let Some(t) = tag {
        p.text(rect.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, t, egui::FontId::proportional(10.5), MUTED);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Encabezado de cada página: categoría pequeña, título y descripción.
pub fn header(ui: &mut Ui, kicker: &str, title: &str, sub: &str) {
    ui.label(RichText::new(kicker.trim_start_matches("› ")).size(12.5).color(MUTED));
    ui.label(RichText::new(title).font(font(26.0)).color(TEXT));
    ui.label(RichText::new(sub).size(13.5).color(MUTED));
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
    ui.painter().line_segment([line.left_center(), pos2(line.right(), line.center().y)], Stroke::new(1.0_f32, PANEL_HI));
    ui.add_space(6.0);
}

/// Título de sección. El color se ignora a propósito: todas las secciones van en el mismo tono.
pub fn section(ui: &mut Ui, title: &str, sub: &str, _color: Color32) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).font(font(15.0)).color(TEXT));
        ui.label(RichText::new(sub).size(12.5).color(MUTED));
    });
}

/// Gráfico de línea con relleno (0–100): los últimos 90 valores.
pub fn sparkline(ui: &mut Ui, data: &[f32], color: Color32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 54.0), Sense::hover());
    let p = ui.painter();
    for k in 1..4 {
        let y = rect.top() + rect.height() * k as f32 / 4.0;
        p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0_f32, alpha(PANEL_HI, 160)));
    }
    if data.len() < 2 {
        return;
    }
    let n = 90usize;
    let step = rect.width() / (n - 1) as f32;
    let start = n.saturating_sub(data.len());
    let pts: Vec<Pos2> = data
        .iter()
        .enumerate()
        .map(|(i, v)| pos2(rect.left() + (start + i) as f32 * step, rect.bottom() - rect.height() * (v / 100.0).clamp(0.0, 1.0)))
        .collect();
    for pt in &pts {
        p.line_segment([*pt, pos2(pt.x, rect.bottom())], Stroke::new(step.max(1.0), alpha(color, 20)));
    }
    p.add(Shape::line(pts, Stroke::new(1.5_f32, color)));
}
