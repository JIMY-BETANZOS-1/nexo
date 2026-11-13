//! Componentes de estilo "cabina mecha": paneles con esquinas cortadas, marcos de
//! cabina, anillos segmentados, barras por bloques y franjas de advertencia.
//! Todo es vectorial (sin imágenes): el ejecutable sigue siendo pequeño.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CursorIcon, Frame, InnerResponse, Margin, Painter, Pos2, Rect,
    Response, RichText, Sense, Shape, Stroke, Ui, Vec2,
};

use super::theme::*;

// ───────────────────────────── geometría ─────────────────────────────

/// Polígono con las esquinas superior-izquierda e inferior-derecha cortadas.
fn chamfer(r: Rect, k: f32) -> Vec<Pos2> {
    let k = k.min(r.width() / 3.0).min(r.height() / 3.0).max(0.0);
    vec![
        pos2(r.left() + k, r.top()),
        r.right_top(),
        pos2(r.right(), r.bottom() - k),
        pos2(r.right() - k, r.bottom()),
        r.left_bottom(),
        pos2(r.left(), r.top() + k),
    ]
}

/// Franjas diagonales de advertencia dentro de `r`.
pub fn hazard(p: &Painter, r: Rect, color: Color32) {
    let p = p.with_clip_rect(r);
    let h = r.height();
    let mut x = r.left() - h;
    while x < r.right() {
        p.add(Shape::convex_polygon(
            vec![pos2(x, r.bottom()), pos2(x + 5.0, r.bottom()), pos2(x + 5.0 + h, r.top()), pos2(x + h, r.top())],
            color,
            Stroke::NONE,
        ));
        x += 11.0;
    }
}

fn hud_shapes(r: Rect, fill: Color32, tint: Option<Color32>, stroke: Stroke, accent: Color32) -> Vec<Shape> {
    let k = 14.0;
    let pts = chamfer(r, k);
    let mut v = vec![Shape::convex_polygon(pts.clone(), fill, Stroke::NONE)];
    if let Some(t) = tint {
        v.push(Shape::convex_polygon(pts.clone(), t, Stroke::NONE));
    }
    v.push(Shape::closed_line(pts, stroke));
    // Marcos de cabina en las esquinas sin cortar.
    let a = Stroke::new(2.0_f32, accent);
    let l = 16.0_f32.min(r.width() / 4.0);
    v.push(Shape::line(vec![pos2(r.right() - l, r.top() + 0.5), pos2(r.right() - 0.5, r.top() + 0.5), pos2(r.right() - 0.5, r.top() + l)], a));
    v.push(Shape::line(vec![pos2(r.left() + 0.5, r.bottom() - l), pos2(r.left() + 0.5, r.bottom() - 0.5), pos2(r.left() + l, r.bottom() - 0.5)], a));
    // Acento en el corte superior.
    v.push(Shape::line_segment([pos2(r.left() + 2.0, r.top() + k - 2.0), pos2(r.left() + k - 2.0, r.top() + 2.0)], Stroke::new(2.0_f32, alpha(accent, 200))));
    v
}

// ───────────────────────────── paneles ─────────────────────────────

/// Panel de cabina. Mismas opciones que un marco normal (relleno, borde, margen).
#[derive(Clone, Copy)]
pub struct Card {
    fill: Color32,
    tint: Option<Color32>,
    stroke: Stroke,
    accent: Color32,
    margin: Margin,
}

impl Card {
    pub fn fill(mut self, c: Color32) -> Self {
        self.fill = c;
        self
    }
    pub fn stroke(mut self, s: Stroke) -> Self {
        self.stroke = s;
        self.accent = s.color;
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
        let accent = if self.accent.a() < 200 { opaque(self.accent) } else { self.accent };
        ui.painter().set(bg, Shape::Vec(hud_shapes(r.response.rect, self.fill, self.tint, self.stroke, accent)));
        r
    }
}

fn opaque(c: Color32) -> Color32 {
    let a = c.a().max(1) as u16;
    let f = |v: u8| ((v as u16 * 255 / a).min(255)) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

pub fn card() -> Card {
    Card {
        fill: alpha(PANEL, 228),
        tint: None,
        stroke: Stroke::new(1.0_f32, alpha(CYAN, 60)),
        accent: alpha(CYAN, 170),
        margin: Margin::same(18.0),
    }
}

pub fn tinted_card(c: Color32) -> Card {
    Card { fill: alpha(PANEL, 228), tint: Some(alpha(c, 22)), stroke: Stroke::new(1.0_f32, alpha(c, 130)), accent: c, margin: Margin::same(18.0) }
}

// ───────────────────────────── fondo y adornos ─────────────────────────────

pub fn background(p: &Painter, rect: Rect) {
    p.rect_filled(rect, 0.0, BG);
    // Brillo holográfico suave.
    for (center, col, rad) in [(rect.right_top() + vec2(-160.0, 120.0), CYAN, 520.0), (rect.left_bottom() + vec2(260.0, -20.0), ORANGE, 360.0)] {
        for i in 0..8 {
            let k = i as f32 / 8.0;
            p.circle_filled(center, rad * (1.0 - k * 0.88), alpha(col, 3));
        }
    }
    // Arcos de radar de fondo.
    let c = rect.right_top() + vec2(-160.0, 120.0);
    for (rr, a0, len) in [(300.0, 0.4, 1.6), (360.0, 2.6, 1.1), (420.0, 4.4, 0.8)] {
        p.add(Shape::line(arc_points(c, rr, a0, a0 + len), Stroke::new(1.0_f32, alpha(CYAN, 16))));
    }
    // Cuadrícula de la cabina.
    let step = 48.0;
    let gc = alpha(CYAN, 7);
    let mut x = rect.left();
    while x < rect.right() {
        p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0_f32, gc));
        x += step;
    }
    let mut y = rect.top();
    while y < rect.bottom() {
        p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0_f32, gc));
        y += step;
    }
    // Líneas de escaneo.
    let sc = Color32::from_black_alpha(34);
    let mut y = rect.top();
    while y < rect.bottom() {
        p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0_f32, sc));
        y += 3.0;
    }
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

/// Indicador circular segmentado, como un medidor de cabina.
pub fn ring(p: &Painter, c: Pos2, r: f32, frac: f32, color: Color32, w: f32) {
    let segs = 48;
    let on = (frac.clamp(0.0, 1.0) * segs as f32).round() as usize;
    let step = TAU / segs as f32;
    for i in 0..segs {
        let a0 = -FRAC_PI_2 + i as f32 * step + step * 0.12;
        let a1 = a0 + step * 0.76;
        let col = if i < on { color } else { alpha(color, 26) };
        p.add(Shape::line(arc_points(c, r, a0, a1), Stroke::new(w, col)));
        if i < on {
            p.add(Shape::line(arc_points(c, r, a0, a1), Stroke::new(w + 6.0, alpha(color, 16))));
        }
    }
    p.circle_stroke(c, r - w * 1.4, Stroke::new(1.0_f32, alpha(color, 70)));
    // Escala exterior y arcos naranjas de cabina.
    for i in 0..12 {
        let a = i as f32 / 12.0 * TAU;
        let d = vec2(a.cos(), a.sin());
        p.line_segment([c + d * (r + w * 0.9), c + d * (r + w * 0.9 + 6.0)], Stroke::new(1.0_f32, alpha(color, 110)));
    }
    let ro = r + w + 10.0;
    for a0 in [0.35_f32, PI + 0.35] {
        p.add(Shape::line(arc_points(c, ro, a0, a0 + 0.9), Stroke::new(2.0_f32, alpha(ORANGE, 190))));
    }
}

/// Radar animado mientras se escanea.
pub fn scanner(p: &Painter, c: Pos2, r: f32, t: f32, color: Color32) {
    for k in [1.0, 0.66, 0.33] {
        p.circle_stroke(c, r * k, Stroke::new(1.0_f32, alpha(color, 45)));
    }
    p.line_segment([c - vec2(r + 8.0, 0.0), c + vec2(r + 8.0, 0.0)], Stroke::new(1.0_f32, alpha(color, 30)));
    p.line_segment([c - vec2(0.0, r + 8.0), c + vec2(0.0, r + 8.0)], Stroke::new(1.0_f32, alpha(color, 30)));
    let a = t * 2.2;
    for k in 0..28 {
        let aa = a - k as f32 * 0.04;
        let av = (130.0 * (1.0 - k as f32 / 28.0)) as u8;
        p.line_segment([c, c + vec2(aa.cos(), aa.sin()) * r], Stroke::new(2.0_f32, alpha(color, av)));
    }
    for (i, (rr, spd, len)) in [(r + 16.0, 1.0f32, 1.3f32), (r + 28.0, -0.7, 0.9), (r * 0.66, 1.7, 0.7)].iter().enumerate() {
        let a0 = t * spd + i as f32;
        let col = if i == 1 { ORANGE } else { color };
        p.add(Shape::line(arc_points(c, *rr, a0, a0 + len), Stroke::new(3.0_f32, col)));
        p.add(Shape::line(arc_points(c, *rr, a0 + PI, a0 + PI + len * 0.5), Stroke::new(2.0_f32, alpha(col, 130))));
    }
    for i in 0..7 {
        let ph = (t * 0.6 + i as f32 * 0.37).fract();
        let ang = i as f32 * 2.1 + t * 0.11;
        let rad = r * (0.2 + 0.75 * ((i * 37 % 10) as f32 / 10.0));
        let q = c + vec2(ang.cos(), ang.sin()) * rad;
        let s = 4.0;
        p.add(Shape::closed_line(vec![q + vec2(0.0, -s), q + vec2(s, 0.0), q + vec2(0.0, s), q + vec2(-s, 0.0)], Stroke::new(1.5_f32, alpha(color, (255.0 * (1.0 - ph)) as u8))));
    }
}

pub fn logo(p: &Painter, c: Pos2, r: f32) {
    let pts: Vec<Pos2> = (0..6)
        .map(|i| {
            let a = i as f32 * TAU / 6.0 - FRAC_PI_2;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    p.add(Shape::closed_line(pts.clone(), Stroke::new(7.0_f32, alpha(CYAN, 35))));
    p.add(Shape::closed_line(pts, Stroke::new(2.0_f32, CYAN)));
    p.circle_stroke(c, r * 0.48, Stroke::new(2.0_f32, ORANGE));
    p.circle_filled(c, r * 0.2, CYAN);
}

// ───────────────────────────── controles ─────────────────────────────

pub fn neon_button(ui: &mut Ui, text: &str, color: Color32, size: Vec2, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let h = ui.ctx().animate_bool(resp.id, enabled && resp.hovered());
    let c = if enabled { color } else { MUTED };
    let p = ui.painter();
    let pts = chamfer(rect, 10.0);
    p.add(Shape::convex_polygon(pts.clone(), alpha(c, (22.0 + 50.0 * h) as u8), Stroke::new(1.5_f32, alpha(c, if enabled { 230 } else { 70 }))));
    if enabled {
        p.add(Shape::closed_line(chamfer(rect.expand(3.0), 12.0), Stroke::new(1.0_f32, alpha(c, (25.0 + 70.0 * h) as u8))));
        // Marca lateral de cabina.
        let side = Rect::from_min_max(pos2(rect.right() - 6.0, rect.top() + 6.0), pos2(rect.right() - 3.0, rect.bottom() - 12.0));
        p.rect_filled(side, 0.0, alpha(c, 200));
    }
    let txt = if enabled { Color32::WHITE.lerp_to_gamma(c, 0.35 - 0.3 * h) } else { MUTED };
    p.text(rect.center(), Align2::CENTER_CENTER, text.to_uppercase(), font(size.y.min(52.0) * 0.34 + 2.0), txt);
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
    let galley = ui.painter().layout_no_wrap(text.to_uppercase(), font(13.5), TEXT);
    let size = galley.size() + vec2(30.0, 14.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let h = ui.ctx().animate_bool(resp.id, resp.hovered() || selected);
    let p = ui.painter();
    let col = if selected { ORANGE } else { CYAN };
    p.add(Shape::convex_polygon(chamfer(rect, 7.0), alpha(col, (8.0 + 40.0 * h) as u8), Stroke::new(1.0_f32, alpha(col, if selected { 230 } else { 80 }))));
    p.text(rect.center(), Align2::CENTER_CENTER, text.to_uppercase(), font(13.5), if selected { ORANGE } else { TEXT });
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Etiqueta de estado con pestaña de color a la izquierda.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_uppercase(), font(10.5), color);
    let size = galley.size() + vec2(18.0, 6.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, alpha(color, 24));
    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, alpha(color, 110)));
    p.rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, color);
    p.galley(rect.center() - galley.size() / 2.0 + vec2(2.0, 0.0), galley, color);
}

pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(54.0, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool(resp.id, *on);
    let col = MUTED.lerp_to_gamma(CYAN, t);
    let p = ui.painter();
    p.add(Shape::convex_polygon(chamfer(rect, 6.0), alpha(col, (22.0 + 40.0 * t) as u8), Stroke::new(1.0_f32, alpha(col, 210))));
    let x = egui::lerp(rect.left() + 13.0..=rect.right() - 13.0, t);
    let knob = Rect::from_center_size(pos2(x, rect.center().y), vec2(18.0, 14.0));
    p.rect_filled(knob, 0.0, col);
    if t > 0.5 {
        p.rect_stroke(knob.expand(2.0), 0.0, Stroke::new(1.0_f32, alpha(CYAN, 90)));
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Barra por bloques.
pub fn bar(ui: &mut Ui, frac: f32, color: Color32, height: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, height.max(5.0)), Sense::hover());
    let p = ui.painter();
    let seg = 7.0;
    let gap = 2.0;
    let n = ((w + gap) / (seg + gap)).floor().max(1.0) as usize;
    let on = (frac.clamp(0.0, 1.0) * n as f32).round() as usize;
    for i in 0..n {
        let x = rect.left() + i as f32 * (seg + gap);
        let r = Rect::from_min_size(pos2(x, rect.top()), vec2(seg, rect.height()));
        p.rect_filled(r, 0.0, if i < on { color } else { alpha(color, 26) });
    }
}

/// Barra de progreso por bloques. Con `None` (avance desconocido) un tramo recorre la barra.
pub fn progress_bar(ui: &mut Ui, frac: Option<f32>, color: Color32, t: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 9.0), Sense::hover());
    let p = ui.painter();
    let seg = 7.0;
    let gap = 2.0;
    let n = ((w + gap) / (seg + gap)).floor().max(1.0) as usize;
    let lit = |i: usize| match frac {
        Some(f) => i < (f.clamp(0.0, 1.0) * n as f32).round() as usize,
        None => {
            let head = ((t * 18.0) as usize) % (n + 8);
            i + 8 > head && i <= head
        }
    };
    for i in 0..n {
        let x = rect.left() + i as f32 * (seg + gap);
        let r = Rect::from_min_size(pos2(x, rect.top()), vec2(seg, rect.height()));
        p.rect_filled(r, 0.0, if lit(i) { color } else { alpha(color, 26) });
    }
    p.rect_stroke(rect.expand(2.0), 0.0, Stroke::new(1.0_f32, alpha(color, 60)));
}

pub fn level_color(frac: f32) -> Color32 {
    if frac >= 0.9 {
        RED
    } else if frac >= 0.75 {
        AMBER
    } else {
        CYAN
    }
}

pub fn stat_card(ui: &mut Ui, icon: &str, title: &str, value: &str, sub: &str, color: Color32, frac: Option<f32>) {
    tinted_card(color).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(112.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon).size(15.0).color(color));
            ui.label(RichText::new(title.to_uppercase()).font(font(12.0)).color(alpha(color, 220)));
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
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    let h = ui.ctx().animate_bool(resp.id, resp.hovered() || selected);
    let p = ui.painter();
    if h > 0.0 {
        p.add(Shape::convex_polygon(chamfer(rect, 9.0), alpha(CYAN, (h * 22.0) as u8), Stroke::NONE));
    }
    if selected {
        p.add(Shape::closed_line(chamfer(rect, 9.0), Stroke::new(1.0_f32, alpha(CYAN, 130))));
        let b = Rect::from_min_size(rect.left_top() + vec2(0.0, 9.0), vec2(4.0, rect.height() - 18.0));
        p.rect_filled(b, 0.0, ORANGE);
        hazard(p, Rect::from_min_max(pos2(rect.right() - 34.0, rect.top() + 4.0), pos2(rect.right() - 6.0, rect.top() + 9.0)), alpha(ORANGE, 200));
    }
    let col = if selected { CYAN } else if tag.is_some() { alpha(TEXT, 130) } else { TEXT };
    p.text(rect.left_center() + vec2(20.0, 0.0), Align2::CENTER_CENTER, icon, egui::FontId::proportional(17.0), col);
    p.text(rect.left_center() + vec2(42.0, 0.0), Align2::LEFT_CENTER, label.to_uppercase(), font(14.0), col);
    if let Some(t) = tag {
        p.text(rect.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, t, font(10.0), alpha(ORANGE, 220));
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn header(ui: &mut Ui, kicker: &str, title: &str, sub: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(34.0, 10.0), Sense::hover());
        hazard(ui.painter(), r, ORANGE);
        ui.label(RichText::new(kicker.trim_start_matches("› ").to_uppercase()).font(font(12.0)).color(ORANGE));
    });
    ui.label(RichText::new(title.to_uppercase()).font(font(30.0)).color(TEXT));
    ui.label(RichText::new(sub).size(13.5).color(MUTED));
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let p = ui.painter();
    p.line_segment([line.left_center(), pos2(line.right(), line.center().y)], Stroke::new(1.0_f32, alpha(CYAN, 40)));
    p.line_segment([line.left_center(), line.left_center() + vec2(120.0, 0.0)], Stroke::new(2.0_f32, CYAN));
    ui.add_space(8.0);
}

pub fn section(ui: &mut Ui, title: &str, sub: &str, color: Color32) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(14.0, 12.0), Sense::hover());
        let p = ui.painter();
        p.add(Shape::convex_polygon(vec![r.left_top(), pos2(r.left() + 6.0, r.top()), pos2(r.right(), r.center().y), pos2(r.left() + 6.0, r.bottom()), r.left_bottom(), pos2(r.left() + 8.0, r.center().y)], color, Stroke::NONE));
        ui.label(RichText::new(title.to_uppercase()).font(font(15.0)).color(color));
        ui.label(RichText::new(sub).size(12.5).color(MUTED));
    });
}

/// Gráfico de línea con relleno (0–100): los últimos 90 valores.
pub fn sparkline(ui: &mut Ui, data: &[f32], color: Color32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 54.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, alpha(color, 10));
    for k in 1..4 {
        let y = rect.top() + rect.height() * k as f32 / 4.0;
        p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0_f32, alpha(color, 18)));
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
    p.add(Shape::line(pts.clone(), Stroke::new(4.0_f32, alpha(color, 50))));
    p.add(Shape::line(pts, Stroke::new(1.5_f32, color)));
}
