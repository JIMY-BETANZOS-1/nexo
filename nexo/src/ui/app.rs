// Proyecto: NEXO
// Archivo: app.rs
// Autor: Chatta Betanzos, Jimy Alexander (jimy-betanzos)

// jimy-betanzos - dependencia: biblioteca estándar de Rust (collections)
use std::collections::VecDeque;
// jimy-betanzos - dependencia: biblioteca estándar de Rust (path)
use std::path::Path;
// jimy-betanzos - dependencia: biblioteca estándar de Rust (sync)
use std::sync::atomic::{AtomicBool, Ordering};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (sync)
use std::sync::mpsc::{channel, Receiver};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (sync)
use std::sync::{Arc, Mutex};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (time)
use std::time::Duration;

// jimy-betanzos - dependencia: ventana e interfaz gráfica (eframe/egui)
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, Layout, Margin, RichText, ScrollArea, Sense, Stroke, Ui,
    ViewportCommand,
};

// jimy-betanzos - dependencia: módulo propio de la interfaz (theme)
use super::theme::*;
// jimy-betanzos - dependencia: módulo propio de la interfaz (thumbcache)
use super::thumbcache::{JbThumbCache, BIG, SMALL};
// jimy-betanzos - dependencia: módulo propio de la interfaz (widgets)
use super::widgets::*;
// jimy-betanzos - dependencia: módulo propio clean
use crate::clean::{self, JbCleanReport};
// jimy-betanzos - dependencia: módulo propio config
use crate::config::{self, JbConfig, JbFrequency, JbHistoryEntry};
// jimy-betanzos - dependencia: módulo propio rules
use crate::rules::{self, JbCategoryDef, JbEnv, JbKind, JbProtect, JbSpecial};
// jimy-betanzos - dependencia: módulo propio procs
use crate::procs::{self, JbAppGroup, JbSampler};
// jimy-betanzos - dependencia: módulo propio scan
use crate::scan::{self, JbCatResult, JbCtx, JbProgress, JbShared};
// jimy-betanzos - dependencia: módulo propio diag
use crate::diag::{self, JbAction, JbReport};
// jimy-betanzos - dependencia: módulo propio
use crate::{adb, fmt, install, notify, schedule, sys, vigia};

/// jimy-betanzos: Resultado de un trabajo en segundo plano: se aplica en el hilo de la interfaz.
type JbApply = Box<dyn FnOnce(&mut JbNexoApp, f64) + Send>;

/// Avance desconocido (barra animada en lugar de porcentaje).
const INDET: u32 = u32::MAX;

/// jimy-betanzos: Operación en curso que se muestra en el panel "En curso".
struct JbOp {
    id: u64,
    label: String,
    frac: Arc<std::sync::atomic::AtomicU32>,
    started: f64,
}

/// jimy-betanzos: Cierre de un programa en curso (ventana de confirmación).
struct JbCloseState {
    g: JbAppGroup,
    sent_at: Option<f64>,
    ack: bool,
    force_ack: bool,
    done: Option<String>,
}

/// jimy-betanzos: Pantallas de la aplicación (Panel, Limpieza, Automático…).
#[derive(Clone, Copy, PartialEq, Eq)]
enum JbPage {
    Panel,
    Clean,
    Auto,
    History,
    Ram,
    Diag,
    Shield,
    Mobile,
}

/// jimy-betanzos: Resultado de simular el modo automático: (bytes, archivos, detalle por categoría).
type JbSimulation = (u64, u64, Vec<(String, u64)>);

/// jimy-betanzos: Trabajo en curso: escaneo, limpieza o simulación.
#[derive(Clone, Copy, PartialEq, Eq)]
enum JbBusy {
    Scan,
    Clean,
    Simulate,
}

/// jimy-betanzos: Mensajes que envían los hilos de trabajo a la ventana.
enum JbMsg {
    Scanned(Vec<JbCatResult>),
    Cleaned(JbCleanReport),
    Simulated(Vec<JbCatResult>),
}

/// jimy-betanzos: Datos del equipo que se muestran en el Panel.
struct JbSnapshot {
    mem: sys::JbMem,
    disk_free: u64,
    disk_total: u64,
    power: sys::JbPower,
    at: f64,
}

/// jimy-betanzos: Toma los datos actuales de disco, memoria y batería.
fn jb_snapshot(now: f64) -> JbSnapshot {
    let (free, total) = sys::jb_disk_space(&sys::jb_system_drive());
    JbSnapshot { mem: sys::jb_memory(), disk_free: free, disk_total: total, power: sys::jb_power(), at: now }
}

/// jimy-betanzos: Estado completo de la ventana de NEXO.
pub struct JbNexoApp {
    page: JbPage,
    env: Arc<JbEnv>,
    defs: Arc<Vec<JbCategoryDef>>,
    admin: bool,
    model: String,
    results: Option<Vec<JbCatResult>>,
    progress: JbShared,
    cancel: Arc<AtomicBool>,
    rx: Option<Receiver<JbMsg>>,
    busy: Option<JbBusy>,
    report: Option<JbCleanReport>,
    confirm: bool,
    ack_protected: bool,
    cfg: JbConfig,
    task_registered: bool,
    history: Vec<JbHistoryEntry>,
    sim: Option<JbSimulation>,
    toast: Option<(String, Color32, f64)>,
    snap: JbSnapshot,
    trim_in: u32,
    was_minimized: bool,
    // Fase 2: memoria
    sampler: Arc<Mutex<JbSampler>>,
    ram_pending: bool,
    jobs: Vec<Receiver<JbApply>>,
    pending_actions: u32,
    cfg_dirty: Option<f64>,
    groups: Vec<JbAppGroup>,
    cpu: f32,
    ram_hist: VecDeque<f32>,
    last_sample: f64,
    close: Option<JbCloseState>,
    vigia_running: bool,
    alerts: Vec<config::JbAlertEntry>,
    // Fase 3: diagnóstico y protección
    diag: Option<JbReport>,
    diag_rx: Option<Receiver<Result<JbReport, String>>>,
    mpscan_rx: Option<Receiver<(Result<i32, String>, u64)>>,
    mpscan_msg: Option<(String, Color32)>,
    // Fase 4: celulares (ADB)
    adb_rx: Option<Receiver<JbAdbMsg>>,
    adb_busy: Option<String>,
    adb_msg: Option<(String, Color32)>,
    adb_scanned: bool,
    devices: Vec<adb::JbDev>,
    dev_sel: Option<String>,
    dev_info: Option<adb::JbInfo>,
    dev_bloat: Vec<adb::JbBloat>,
    dev_usage: Vec<(String, f64)>,
    pair_addr: String,
    pair_code: String,
    conn_addr: String,
    uninstall_armed: bool,
    thumbs: JbThumbCache,
    ops: Vec<JbOp>,
    next_op: u64,
    now: f64,
    busy_started: f64,
    diag_started: f64,
    diag_prog: Arc<std::sync::atomic::AtomicU32>,
    mp_started: f64,
    adb_started: f64,
}

/// jimy-betanzos: Mensajes de las tareas con celulares.
enum JbAdbMsg {
    Devices(Result<Vec<adb::JbDev>, String>),
    Loaded(String, Result<adb::JbInfo, String>, Vec<adb::JbBloat>, Vec<(String, f64)>),
    Done(Result<String, String>, bool),
}

/// jimy-betanzos: Métodos de NexoApp.
impl JbNexoApp {
    /// jimy-betanzos: Prepara la ventana, el estilo y los datos iniciales.
    pub fn jb_new(cc: &eframe::CreationContext, start_page: Option<String>) -> Self {
        jb_setup(&cc.egui_ctx);
        let env = JbEnv::jb_detect();
        let defs = rules::jb_categories(&env);
        let page = match start_page.as_deref() {
            Some("ram") => JbPage::Ram,
            Some("limpieza") => JbPage::Clean,
            Some("diagnostico") => JbPage::Diag,
            Some("proteccion") => JbPage::Shield,
            Some("celulares") => JbPage::Mobile,
            Some("automatico") => JbPage::Auto,
            Some("historial") => JbPage::History,
            _ => JbPage::Panel,
        };
        let mut app = JbNexoApp {
            page,
            env: Arc::new(env),
            defs: Arc::new(defs),
            admin: sys::jb_is_admin(),
            model: sys::jb_machine_model(),
            results: None,
            progress: Arc::new(Mutex::new(JbProgress::default())),
            cancel: Arc::new(AtomicBool::new(false)),
            rx: None,
            busy: None,
            report: None,
            confirm: false,
            ack_protected: false,
            cfg: JbConfig::jb_load(),
            task_registered: false,
            history: config::jb_load_history(),
            sim: None,
            toast: None,
            snap: jb_snapshot(0.0),
            trim_in: 4,
            was_minimized: false,
            sampler: Arc::new(Mutex::new(JbSampler::default())),
            ram_pending: false,
            jobs: Vec::new(),
            pending_actions: 0,
            cfg_dirty: None,
            groups: Vec::new(),
            cpu: 0.0,
            ram_hist: VecDeque::new(),
            last_sample: -10.0,
            close: None,
            vigia_running: vigia::jb_is_running(),
            alerts: Vec::new(),
            diag: diag::jb_load(),
            diag_rx: None,
            mpscan_rx: None,
            mpscan_msg: None,
            adb_rx: None,
            adb_busy: None,
            adb_msg: None,
            adb_scanned: false,
            devices: Vec::new(),
            dev_sel: None,
            dev_info: None,
            dev_bloat: Vec::new(),
            dev_usage: Vec::new(),
            pair_addr: String::new(),
            pair_code: String::new(),
            conn_addr: String::new(),
            uninstall_armed: false,
            thumbs: JbThumbCache::jb_new(),
            ops: Vec::new(),
            next_op: 1,
            now: 0.0,
            busy_started: 0.0,
            diag_started: 0.0,
            diag_prog: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            mp_started: 0.0,
            adb_started: 0.0,
        };
        app.jb_check_task();
        // Si ya estaba instalado con una versión anterior, se actualiza sin molestar.
        app.jb_bg(|| {
            if install::jb_needs_update() {
                let _ = install::jb_install();
            }
            Box::new(|_: &mut JbNexoApp, _| {})
        });
        app
    }

    /// jimy-betanzos: Comprueba si la tarea programada sigue registrada.
    fn jb_check_task(&mut self) {
        self.jb_bg(|| {
            let reg = schedule::jb_is_registered();
            Box::new(move |a: &mut JbNexoApp, _| a.task_registered = reg)
        });
    }

    /// jimy-betanzos: Ejecuta `work` en otro hilo; lo que devuelve se aplica luego en la interfaz.
    fn jb_bg(&mut self, work: impl FnOnce() -> JbApply + Send + 'static) {
        let (tx, rx) = channel();
        self.jobs.push(rx);
        std::thread::spawn(move || {
            sys::jb_interactive_thread();
            let _ = tx.send(work());
        });
    }

    /// jimy-betanzos: Igual que `bg`, para acciones del usuario: aparecen en el panel "En curso" y, mientras
    /// corren, sus botones se desactivan. `work` recibe dónde informar su avance (milésimas).
    fn jb_bg_action(&mut self, label: &str, work: impl FnOnce(Arc<std::sync::atomic::AtomicU32>) -> JbApply + Send + 'static) {
        self.pending_actions += 1;
        let id = self.next_op;
        self.next_op += 1;
        let frac = Arc::new(std::sync::atomic::AtomicU32::new(INDET));
        self.ops.push(JbOp { id, label: label.to_string(), frac: frac.clone(), started: self.now });
        self.jb_bg(move || {
            let f = work(frac);
            Box::new(move |a: &mut JbNexoApp, now| {
                a.pending_actions = a.pending_actions.saturating_sub(1);
                a.ops.retain(|o| o.id != id);
                f(a, now)
            })
        });
    }

    /// jimy-betanzos: Panel "En curso": todo lo que NEXO está haciendo, con su avance y tiempo.
    fn jb_ops_view(&mut self, ctx: &egui::Context, now: f64) {
        // jimy-betanzos - dependencia: biblioteca estándar de Rust (sync)
        use std::sync::atomic::Ordering;
        let mut rows: Vec<(String, Option<f32>, f64, Color32)> = Vec::new();
        if let Some(b) = self.busy {
            let p = self.progress.lock().map(|p| p.clone()).unwrap_or_default();
            let frac = if b == JbBusy::Clean && p.total_bytes > 0 {
                (p.bytes as f32 / p.total_bytes as f32).min(0.99)
            } else if p.steps > 0 {
                (p.step as f32 / p.steps as f32).min(0.99)
            } else {
                0.0
            };
            let label = match b {
                JbBusy::Scan => format!("Escaneando · {} archivos", fmt::jb_count(p.files)),
                JbBusy::Clean => format!("Limpiando · {} liberados", fmt::jb_bytes(p.bytes)),
                JbBusy::Simulate => "Simulando la limpieza automática".to_string(),
            };
            rows.push((label, Some(frac), self.busy_started, ACCENT));
        }
        if self.diag_rx.is_some() {
            let f = self.diag_prog.load(Ordering::Relaxed) as f32 / 1000.0;
            rows.push(("Diagnóstico y protección".into(), Some(f), self.diag_started, ACCENT));
        }
        if self.mpscan_rx.is_some() {
            let est = if self.cfg.defender_scan_secs > 0 { self.cfg.defender_scan_secs as f64 } else { 240.0 };
            let f = ((now - self.mp_started) / est).min(0.95) as f32;
            rows.push((format!("Análisis de {} (tiempo estimado)", diag::jb_txt("av")), Some(f), self.mp_started, ACCENT));
        }
        if let Some(b) = &self.adb_busy {
            rows.push((b.clone(), None, self.adb_started, ACCENT));
        }
        for op in &self.ops {
            let v = op.frac.load(Ordering::Relaxed);
            rows.push((op.label.clone(), if v == INDET { None } else { Some(v as f32 / 1000.0) }, op.started, ACCENT));
        }
        if rows.is_empty() {
            return;
        }
        let t = now as f32;
        egui::Area::new(egui::Id::new("ops"))
            .order(egui::Order::Foreground)
            .anchor(Align2::RIGHT_TOP, vec2(-22.0, 50.0))
            .interactable(false)
            .show(ctx, |ui| {
                jb_tinted_card(ACCENT).jb_fill(jb_alpha(PANEL, 245)).jb_inner_margin(Margin::same(12.0)).jb_show(ui, |ui| {
                    ui.set_width(330.0);
                    ui.label(RichText::new("En curso").font(jb_font(11.0)).color(ACCENT));
                    for (label, frac, started, col) in rows {
                        let elapsed = (now - started).max(0.0);
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(RichText::new(label).size(12.5)).truncate());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let txt = frac.map(|f| format!("{:.0} %", f * 100.0)).unwrap_or_else(|| "…".into());
                                ui.label(RichText::new(txt).font(jb_font(13.0)).color(col));
                            });
                        });
                        jb_progress_bar(ui, frac, col, t);
                        let eta = match frac {
                            Some(f) if f > 0.04 && elapsed > 1.0 => format!(" · quedan ≈ {}", fmt::jb_secs(elapsed * (1.0 - f as f64) / f as f64)),
                            _ => String::new(),
                        };
                        ui.label(RichText::new(format!("{} transcurridos{}", fmt::jb_secs(elapsed), eta)).size(11.0).color(MUTED));
                    }
                });
            });
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    /// jimy-betanzos: Revisa el avance de los trabajos en segundo plano.
    fn jb_poll_jobs(&mut self, now: f64) {
        let mut done: Vec<JbApply> = Vec::new();
        self.jobs.retain(|rx| match rx.try_recv() {
            Ok(f) => {
                done.push(f);
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(_) => false,
        });
        for f in done {
            f(self, now);
        }
    }

    /// jimy-betanzos: Muestra un aviso breve en la parte inferior.
    fn jb_show_toast(&mut self, text: String, color: Color32, now: f64) {
        self.toast = Some((text, color, now + 5.0));
    }

    /// jimy-betanzos: Recibe los resultados del escaneo y la limpieza.
    fn jb_poll(&mut self, now: f64) {
        let msg = self.rx.as_ref().and_then(|rx| rx.try_recv().ok());
        let Some(msg) = msg else { return };
        self.rx = None;
        self.busy = None;
        self.trim_in = 3;
        match msg {
            JbMsg::Scanned(r) => self.results = Some(r),
            JbMsg::Simulated(r) => {
                let safe = r.iter().filter(|c| c.kind == JbKind::Safe);
                let freed = safe.clone().map(|c| c.size).sum();
                let files = safe.clone().map(|c| c.count).sum();
                let per = safe.filter(|c| c.size > 0).map(|c| (c.name.to_string(), c.size)).collect();
                self.sim = Some((freed, files, per));
            }
            JbMsg::Cleaned(rep) => {
                self.history = config::jb_push_history(JbHistoryEntry {
                    ts: fmt::jb_now(),
                    mode: "manual".into(),
                    freed: rep.freed,
                    files: rep.files,
                    skipped: rep.skipped,
                    recycled: rep.recycled,
                    details: rep.per_cat.clone(),
                    ..Default::default()
                });
                self.jb_show_toast(format!("Limpieza completada · {} liberados", fmt::jb_bytes(rep.freed)), ACCENT, now);
                self.report = Some(rep);
                self.results = None;
                self.snap = jb_snapshot(now);
            }
        }
    }

    /// jimy-betanzos: Lanza el escaneo en un hilo aparte.
    fn jb_spawn_scan(&mut self, only: Option<Vec<String>>, simulate: bool) {
        if self.busy.is_some() {
            return;
        }
        let (defs, prog, cancel, admin) = (self.defs.clone(), self.progress.clone(), self.cancel.clone(), self.admin);
        cancel.store(false, Ordering::Relaxed);
        if let Ok(mut p) = prog.lock() {
            *p = JbProgress { label: "Preparando escaneo".into(), ..Default::default() };
        }
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.busy = Some(if simulate { JbBusy::Simulate } else { JbBusy::Scan });
        self.busy_started = self.now;
        if !simulate {
            self.report = None;
            self.results = None;
        }
        std::thread::spawn(move || {
            sys::jb_interactive_thread();
            let procs = sys::jb_running_processes();
            let ctx = JbCtx { admin, procs: &procs, progress: &prog, cancel: &cancel };
            let r = scan::jb_scan_all(&defs, only.as_deref(), &ctx);
            let _ = tx.send(if simulate { JbMsg::Simulated(r) } else { JbMsg::Scanned(r) });
        });
    }

    /// jimy-betanzos: Lanza la limpieza en un hilo aparte.
    fn jb_spawn_clean(&mut self) {
        let Some(results) = self.results.clone() else { return };
        if self.busy.is_some() {
            return;
        }
        let (defs, env, prog, cancel, admin) =
            (self.defs.clone(), self.env.clone(), self.progress.clone(), self.cancel.clone(), self.admin);
        cancel.store(false, Ordering::Relaxed);
        let total: u64 = results.iter().map(|r| r.jb_selected_size()).sum();
        if let Ok(mut p) = prog.lock() {
            *p = JbProgress { label: "Iniciando limpieza".into(), total_bytes: total, ..Default::default() };
        }
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.busy = Some(JbBusy::Clean);
        self.busy_started = self.now;
        std::thread::spawn(move || {
            sys::jb_interactive_thread();
            let procs = sys::jb_running_processes();
            let ctx = JbCtx { admin, procs: &procs, progress: &prog, cancel: &cancel };
            let rep = clean::jb_clean(&defs, &results, &env, &ctx);
            let _ = tx.send(JbMsg::Cleaned(rep));
        });
    }

    // marco de la ventana

    /// jimy-betanzos: Dibuja la barra de título con los botones de ventana.
    fn jb_titlebar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("titlebar")
            .exact_height(40.0)
            .show_separator_line(false)
            .frame(egui::Frame::none().fill(jb_alpha(BG, 210)).inner_margin(Margin::symmetric(16.0, 0.0)))
            .show(ctx, |ui| {
                let drag = ui.interact(ui.max_rect(), egui::Id::new("drag"), Sense::click_and_drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                if drag.double_clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                }
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("NEXO").font(jb_font(13.0)).color(ACCENT));
                    ui.label(RichText::new("Control local del sistema").font(jb_font(12.0)).color(MUTED));
                    ui.add_space(14.0);
                    let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 3.5, ACCENT);
                    ui.painter().circle_filled(r.center(), 7.0, jb_alpha(ACCENT, 40));
                    ui.label(RichText::new("Sin conexión · 100 % local").font(jb_font(11.0)).color(jb_alpha(ACCENT, 220)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if jb_title_btn(ui, "×", DANGER).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                        if jb_title_btn(ui, if maximized { "❐" } else { "☐" }, ACCENT).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                        }
                        if jb_title_btn(ui, "—", ACCENT).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                        }
                    });
                });
            });
    }

    /// jimy-betanzos: Dibuja el menú lateral.
    fn jb_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("side")
            .exact_width(240.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::none()
                    .fill(jb_alpha(PANEL, 175))
                    .stroke(Stroke::new(1.0_f32, jb_alpha(ACCENT, 28)))
                    .inner_margin(Margin { left: 14.0, right: 14.0, top: 14.0, bottom: 14.0 }),
            )
            .show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::hover());
                let p = ui.painter();
                jb_logo(p, r.left_center() + vec2(28.0, 0.0), 21.0);
                p.text(r.left_center() + vec2(62.0, -9.0), Align2::LEFT_CENTER, "NEXO", jb_font(27.0), TEXT);
                p.text(r.left_center() + vec2(63.0, 17.0), Align2::LEFT_CENTER, "Optimización inteligente", jb_font(9.5), ACCENT);
                ui.add_space(12.0);

                let pages: [(JbPage, &str, &str, Option<&str>); 8] = [
                    (JbPage::Panel, "🏠", "Panel", None),
                    (JbPage::Clean, "🗑", "Limpieza", None),
                    (JbPage::Auto, "⏰", "Automático", None),
                    (JbPage::History, "📜", "Historial", None),
                    (JbPage::Ram, "📊", "Memoria RAM", None),
                    (JbPage::Diag, "🔍", "Diagnóstico", None),
                    (JbPage::Shield, "🔒", "Protección", None),
                    (JbPage::Mobile, "📱", "Celulares", None),
                ];
                for (pg, icon, label, tag) in pages {
                    if jb_nav_item(ui, icon, label, self.page == pg, tag).clicked() {
                        self.page = pg;
                        if pg == JbPage::Auto {
                            self.jb_check_task();
                        }
                    }
                }

                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"), " · sin conexión a internet")).size(11.0).color(MUTED));
                    let (txt, col) = if cfg!(unix) { ("• Modo usuario", ACCENT) } else if self.admin { ("• Acceso total", ACCENT) } else { ("• Acceso limitado", WARN) };
                    jb_tinted_card(col).jb_inner_margin(Margin::symmetric(12.0, 8.0)).jb_show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(txt).font(jb_font(12.0)).color(col));
                    });
                });
            });
    }

    // páginas

    /// jimy-betanzos: Pantalla Panel: estado del equipo.
    fn jb_page_panel(&mut self, ui: &mut Ui) {
        jb_header(ui, "› Centro de control", &self.model, "Estado de tu equipo en tiempo real. Todo se analiza aquí mismo, sin internet.");
        let s = &self.snap;
        let disk_used = 1.0 - s.disk_free as f32 / s.disk_total.max(1) as f32;
        let mem = s.mem.load as f32 / 100.0;
        let (disk_free, disk_total) = (s.disk_free, s.disk_total);
        let (mem_load, mem_avail, mem_total) = (s.mem.load, s.mem.avail, s.mem.total);
        let (on_ac, pct, has_bat) = (s.power.on_ac, s.power.percent, s.power.has_battery);
        let auto_on = self.cfg.auto_enabled && self.task_registered;
        let auto_sub = format!("{} · {:02}:00", self.cfg.frequency.jb_label(), self.cfg.hour);
        let last = self.history.iter().rev().find(|h| h.mode != "pospuesta").cloned();
        let health = self.diag.as_ref().map(|d| (d.overall, d.ts));
        let idle = self.busy.is_none();
        let mut scan_now = false;

        ui.columns(4, |c| {
            jb_stat_card(&mut c[0], "💾", "Disco del sistema", &format!("{} libres", fmt::jb_bytes(disk_free)),
                &format!("de {} · {:.0} % usado", fmt::jb_bytes(disk_total), disk_used * 100.0), jb_level_color(disk_used), Some(disk_used));
            jb_stat_card(&mut c[1], "📊", "Memoria RAM", &format!("{} %", mem_load),
                &format!("{} libres de {}", fmt::jb_bytes(mem_avail), fmt::jb_bytes(mem_total)), jb_level_color(mem), Some(mem));
            let bat = pct.unwrap_or(100) as f32 / 100.0;
            let bcol = if bat < 0.3 && !on_ac { WARN } else { ACCENT };
            jb_stat_card(&mut c[2], "🔋", "Energía", if on_ac { "Enchufado" } else { "Con batería" },
                &if has_bat { format!("{} % de carga", pct.unwrap_or(0)) } else { "Sin batería detectada".into() }, bcol, has_bat.then_some(bat));
            jb_stat_card(&mut c[3], "⏰", "Modo automático", if auto_on { "Activo" } else { "Apagado" },
                &if auto_on { auto_sub.clone() } else { "Configúralo en Automático".into() }, if auto_on { ACCENT } else { MUTED }, None);
        });
        ui.add_space(10.0);

        ui.columns(2, |c| {
            jb_card().jb_show(&mut c[0], |ui| {
                ui.set_width(ui.available_width());
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 250.0), Sense::hover());
                let col = jb_level_color(disk_used);
                jb_ring(ui.painter(), r.center(), 88.0, disk_used, col, 10.0);
                ui.painter().text(r.center() - vec2(0.0, 10.0), Align2::CENTER_CENTER, format!("{:.0}%", disk_used * 100.0), jb_font(44.0), TEXT);
                ui.painter().text(r.center() + vec2(0.0, 26.0), Align2::CENTER_CENTER, "Disco usado", jb_font(12.0), col);
                ui.vertical_centered(|ui| {
                    if disk_used >= 0.75 {
                        ui.label(RichText::new("Tu disco está por encima del 75 %. Una limpieza puede ayudar.").size(13.0).color(WARN));
                    } else {
                        ui.label(RichText::new("Espacio en buen estado.").size(13.0).color(MUTED));
                    }
                    ui.add_space(6.0);
                    if jb_button(ui, "🔍  Escanear ahora", ACCENT, vec2(ui.available_width().min(340.0), 48.0), idle).clicked() {
                        scan_now = true;
                    }
                });
            });
            let ui = &mut c[1];
            jb_tinted_card(ACCENT).jb_show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("🛡  Garantías de NEXO").font(jb_font(15.0)).color(ACCENT));
                ui.add_space(4.0);
                for g in [
                    "Videos y documentos: jamás se borran solos.",
                    "Capturas y descargas van a la Papelera: siempre recuperables.",
                    "Solo se elimina basura: temporales, caché y registros.",
                    "Siempre ves lo que se va a borrar antes de confirmar.",
                    "Trabaja con prioridad baja: no frena tu equipo.",
                    "100 % local: no envía nada a internet.",
                ] {
                    ui.label(RichText::new(format!("✔  {}", g)).size(13.5));
                }
            });
            ui.add_space(8.0);
            jb_card().jb_show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Última actividad").font(jb_font(12.0)).color(MUTED));
                match &last {
                    Some(h) => {
                        ui.label(RichText::new(format!("{} liberados", fmt::jb_bytes(h.freed))).font(jb_font(24.0)).color(ACCENT));
                        ui.label(RichText::new(format!(
                            "Limpieza {} · {} · {} archivos",
                            if h.mode == "auto" { "automática" } else { "manual" },
                            fmt::jb_ago(h.ts),
                            fmt::jb_count(h.files)
                        )).size(12.5).color(MUTED));
                    }
                    None => {
                        ui.label(RichText::new("Aún no hay limpiezas registradas.").size(13.5).color(MUTED));
                    }
                }
                ui.add_space(6.0);
                ui.label(RichText::new("Salud del equipo").font(jb_font(12.0)).color(MUTED));
                match health {
                    Some((score, ts)) => {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{}/100", score)).font(jb_font(24.0)).color(jb_score_color(score)));
                            ui.label(RichText::new(format!("{} · {}", diag::jb_grade(score), fmt::jb_ago(ts))).size(12.5).color(MUTED));
                        });
                    }
                    None => {
                        ui.label(RichText::new("Sin diagnóstico todavía: hazlo en la pestaña Diagnóstico.").size(13.5).color(MUTED));
                    }
                }
            });
        });

        if scan_now {
            self.page = JbPage::Clean;
            self.jb_spawn_scan(None, false);
        }
    }

    /// jimy-betanzos: Aviso cuando falta el permiso de administrador.
    fn jb_admin_banner(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.admin || cfg!(unix) {
            return;
        }
        jb_tinted_card(WARN).jb_inner_margin(Margin::symmetric(16.0, 10.0)).jb_show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("⚡").color(WARN).size(16.0));
                ui.label(RichText::new("Rechazaste el permiso de Windows: se omiten Windows Update, temporales de Windows y registros del sistema.").size(13.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if jb_link(ui, "Volver a pedir permiso", WARN).clicked() && sys::jb_relaunch_as_admin() {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                });
            });
        });
        ui.add_space(6.0);
    }

    /// jimy-betanzos: Pantalla Limpieza.
    fn jb_page_clean(&mut self, ui: &mut Ui, t: f32) {
        jb_header(ui, "› Limpieza inteligente", "Buscar y eliminar basura", "Nada se borra sin que lo veas primero. Tus videos y documentos quedan protegidos.");
        let ctx = ui.ctx().clone();
        self.jb_admin_banner(ui, &ctx);
        match self.busy {
            Some(JbBusy::Scan) | Some(JbBusy::Clean) => self.jb_busy_view(ui, t),
            _ if self.results.is_some() => self.jb_results_view(ui),
            _ => self.jb_idle_view(ui),
        }
    }

    /// jimy-betanzos: Vista mientras se escanea o limpia.
    fn jb_busy_view(&mut self, ui: &mut Ui, t: f32) {
        let p = self.progress.lock().map(|p| p.clone()).unwrap_or_default();
        let cleaning = self.busy == Some(JbBusy::Clean);
        let col = ACCENT;
        let mut cancel = false;
        jb_card().jb_show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 300.0), Sense::hover());
            jb_scanner(ui.painter(), r.center(), 112.0, t, col);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(if cleaning { "Limpiando…" } else { "Escaneando…" }).font(jb_font(26.0)).color(col));
                let frac = if cleaning && p.total_bytes > 0 {
                    (p.bytes as f32 / p.total_bytes as f32).min(0.99)
                } else if p.steps > 0 {
                    (p.step as f32 / p.steps as f32).min(0.99)
                } else {
                    0.0
                };
                ui.label(RichText::new(format!("{:.0} %", frac * 100.0)).font(jb_font(34.0)).color(TEXT));
                ui.label(RichText::new(p.label.clone()).size(14.0));
                ui.add_space(4.0);
                ui.allocate_ui(vec2(460.0, 12.0), |ui| jb_progress_bar(ui, Some(frac), col, t));
                let elapsed = (self.now - self.busy_started).max(0.0);
                let eta = if frac > 0.04 && elapsed > 1.0 { format!(" · quedan ≈ {}", fmt::jb_secs(elapsed * (1.0 - frac as f64) / frac as f64)) } else { String::new() };
                ui.label(RichText::new(format!("{} transcurridos{}", fmt::jb_secs(elapsed), eta)).size(12.5).color(MUTED));
                let line = if cleaning {
                    format!("{} archivos eliminados · {} liberados", fmt::jb_count(p.files), fmt::jb_bytes(p.bytes))
                } else {
                    format!("{} archivos analizados", fmt::jb_count(p.files))
                };
                ui.label(RichText::new(line).size(13.0).color(MUTED));
                ui.label(RichText::new(fmt::jb_short_path(Path::new(&p.current), 96)).monospace().size(11.0).color(jb_alpha(MUTED, 170)));
                ui.add_space(8.0);
                if jb_button(ui, "Cancelar", DANGER, vec2(170.0, 38.0), true).clicked() {
                    cancel = true;
                }
            });
        });
        if cancel {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// jimy-betanzos: Vista inicial de Limpieza («Listo para escanear»).
    fn jb_idle_view(&mut self, ui: &mut Ui) {
        if let Some(rep) = &self.report {
            jb_tinted_card(ACCENT).jb_show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("✔  Limpieza completada").font(jb_font(18.0)).color(ACCENT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(fmt::jb_bytes(rep.freed)).font(jb_font(30.0)).color(ACCENT));
                    });
                });
                ui.label(format!("{} archivos eliminados o movidos.", fmt::jb_count(rep.files)));
                if rep.recycled > 0 {
                    ui.label(RichText::new(format!("♻ {} elementos enviados a la Papelera: puedes recuperarlos desde allí.", fmt::jb_count(rep.recycled))).color(WARN));
                }
                if rep.skipped > 0 {
                    ui.label(RichText::new(format!("{} archivos estaban en uso y se dejaron intactos.", fmt::jb_count(rep.skipped))).color(MUTED));
                }
                ui.horizontal_wrapped(|ui| {
                    for (n, b) in rep.per_cat.iter().filter(|(_, b)| *b > 0) {
                        jb_badge(ui, &format!("{} · {}", n, fmt::jb_bytes(*b)), ACCENT);
                    }
                });
            });
            ui.add_space(8.0);
        }
        let mut go = false;
        jb_card().jb_show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 260.0), Sense::hover());
            jb_scanner(ui.painter(), r.center(), 100.0, 0.8, jb_alpha(ACCENT, 170));
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Listo para escanear").font(jb_font(22.0)));
                ui.label(RichText::new("El escaneo solo mira: no borra nada. Después tú eliges qué limpiar.").size(13.5).color(MUTED));
                ui.add_space(8.0);
                if jb_button(ui, "🔍  Iniciar escaneo", ACCENT, vec2(300.0, 50.0), true).clicked() {
                    go = true;
                }
            });
        });
        if go {
            self.jb_spawn_scan(None, false);
        }
    }

    /// jimy-betanzos: Vista con los resultados del escaneo por categoría.
    fn jb_results_view(&mut self, ui: &mut Ui) {
        let admin = self.admin;
        let mut open_confirm = false;
        let mut rescan = false;
        let Some(results) = self.results.as_mut() else { return };

        let safe: u64 = results.iter().filter(|r| r.kind == JbKind::Safe).map(|r| r.size).sum();
        let safe_n: u64 = results.iter().filter(|r| r.kind == JbKind::Safe).map(|r| r.count).sum();
        let review: u64 = results.iter().filter(|r| r.kind == JbKind::Review).map(|r| r.size).sum();
        let (prot, prot_n) = results
            .iter()
            .find(|r| r.kind == JbKind::Protected)
            .map(|r| (r.size, r.count))
            .unwrap_or((0, 0));
        // Resumen compacto en una sola franja: deja más altura para revisar archivos.
        jb_card().jb_inner_margin(Margin::symmetric(18.0, 10.0)).jb_show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.columns(3, |c| {
                for (k, (title, value, sub, col)) in [
                    ("Basura eliminable", fmt::jb_bytes(safe), format!("{} temporales y caché", fmt::jb_count(safe_n)), ACCENT),
                    ("Para revisar", fmt::jb_bytes(review), "Papelera, capturas, instaladores".to_string(), WARN),
                    ("Videos y documentos", fmt::jb_bytes(prot), format!("{} protegidos", fmt::jb_count(prot_n)), ACCENT),
                ]
                .into_iter()
                .enumerate()
                {
                    c[k].horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(4.0, 40.0), Sense::hover());
                        ui.painter().rect_filled(r, 0.0, col);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(title).font(jb_font(11.0)).color(col));
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(value).font(jb_font(22.0)).color(TEXT));
                                ui.label(RichText::new(sub).size(11.5).color(MUTED));
                            });
                        });
                    });
                }
            });
        });
        ui.add_space(6.0);

        let list_h = (ui.available_height() - 84.0).max(160.0);
        let thumbs = &mut self.thumbs;
        ScrollArea::vertical().max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
            for (kind, title, sub, col) in [
                (JbKind::Safe, "Limpieza segura", "basura del sistema · se elimina definitivamente", ACCENT),
                (JbKind::Review, "Para revisar", "van a la Papelera de reciclaje · recuperables", WARN),
                (JbKind::Protected, "Protegidos", "videos y documentos · nunca en automático · a la Papelera", ACCENT),
            ] {
                jb_section(ui, title, sub, col);
                for r in results.iter_mut().filter(|r| r.kind == kind) {
                    jb_category_card(ui, r, admin, thumbs);
                }
            }
        });

        let sel: u64 = results.iter().map(|r| r.jb_selected_size()).sum();
        let sel_cats = results.iter().filter(|r| r.jb_selected_size() > 0).count();
        jb_card().jb_inner_margin(Margin::symmetric(18.0, 12.0)).jb_show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Seleccionado").font(jb_font(11.0)).color(MUTED));
                    ui.label(RichText::new(fmt::jb_bytes(sel)).font(jb_font(26.0)).color(if sel > 0 { ACCENT } else { MUTED }));
                });
                ui.label(RichText::new(format!("en {} categorías", sel_cats)).color(MUTED));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if jb_button(ui, "🗑  Limpiar selección", ACCENT, vec2(250.0, 44.0), sel > 0).clicked() {
                        open_confirm = true;
                    }
                    if jb_button(ui, "⟳  Re-escanear", ACCENT, vec2(170.0, 44.0), true).clicked() {
                        rescan = true;
                    }
                });
            });
        });

        if open_confirm {
            self.confirm = true;
            self.ack_protected = false;
        }
        if rescan {
            self.jb_spawn_scan(None, false);
        }
    }

    /// jimy-betanzos: Pantalla Automático: programación de la limpieza.
    fn jb_page_auto(&mut self, ui: &mut Ui, t: f32) {
        jb_header(ui, "› Piloto automático", "Limpieza automática", &format!("{} abre NEXO a la hora elegida, limpia la basura y lo cierra. En reposo consume 0 MB de RAM.", diag::jb_txt("os")));
        let mut apply = false;
        let mut simulate = false;
        let idle = self.busy.is_none() && self.pending_actions == 0;
        let simulating = self.busy == Some(JbBusy::Simulate);
        let last_auto = self.history.iter().rev().find(|h| h.mode == "auto" || h.mode == "pospuesta").cloned();
        let admin = self.admin;
        let registered = self.task_registered;
        let defs = self.defs.clone();
        let armed = self.uninstall_armed;
        let mut uninstall_click = false;
        let cfg = &mut self.cfg;
        let sim = &self.sim;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |c| {
                c[0].with_layout(Layout::top_down(Align::Min), |ui| {
                jb_card().jb_show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        jb_toggle(ui, &mut cfg.auto_enabled);
                        ui.label(RichText::new("Limpieza automática").font(jb_font(19.0)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if registered {
                                jb_badge(ui, "Programada", ACCENT);
                            } else {
                                jb_badge(ui, "No programada", MUTED);
                            }
                        });
                    });
                    ui.add_space(10.0);
                    ui.label(RichText::new("Frecuencia").font(jb_font(12.0)).color(MUTED));
                    ui.horizontal(|ui| {
                        for f in [JbFrequency::Daily, JbFrequency::Weekly, JbFrequency::Monthly] {
                            if jb_chip(ui, f.jb_label(), cfg.frequency == f).clicked() {
                                cfg.frequency = f;
                            }
                        }
                    });
                    ui.label(RichText::new(match cfg.frequency {
                        JbFrequency::Daily => "Todos los días.",
                        JbFrequency::Weekly => "Cada domingo.",
                        JbFrequency::Monthly => "El día 1 de cada mes.",
                    }).size(12.5).color(MUTED));
                    ui.add_space(6.0);
                    ui.label(RichText::new("Hora").font(jb_font(12.0)).color(MUTED));
                    egui::ComboBox::from_id_salt("hour")
                        .selected_text(format!("{:02}:00", cfg.hour))
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for h in 0..24u8 {
                                ui.selectable_value(&mut cfg.hour, h, format!("{:02}:00", h));
                            }
                        });
                    ui.label(RichText::new("Si el equipo está apagado a esa hora, se hará en cuanto lo enciendas.").size(12.5).color(MUTED));
                    ui.add_space(8.0);
                    ui.label(RichText::new("Qué limpia · solo basura del sistema").font(jb_font(12.0)).color(MUTED));
                    for d in rules::jb_auto_eligible(&defs) {
                        let mut on = cfg.auto_categories.iter().any(|x| x == d.id);
                        let label = if d.jb_admin_only() && !admin {
                            format!("{}   · requiere administrador", d.name)
                        } else {
                            d.name.to_string()
                        };
                        if ui.checkbox(&mut on, label).changed() {
                            if on {
                                cfg.auto_categories.push(d.id.to_string());
                            } else {
                                cfg.auto_categories.retain(|x| x != d.id);
                            }
                        }
                    }
                    if !admin {
                        ui.label(RichText::new("Para incluir las categorías de administrador, abre NEXO como administrador y vuelve a aplicar.").size(12.0).color(WARN));
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if jb_button(ui, "💾  Guardar y aplicar", ACCENT, vec2(220.0, 44.0), idle).clicked() {
                            apply = true;
                        }
                        if jb_button(ui, "▶  Simular ahora", ACCENT, vec2(190.0, 44.0), idle).clicked() {
                            simulate = true;
                        }
                    });
                });
                });

                // Columna sin justificar: evita que el texto se estire palabra a palabra.
                c[1].with_layout(Layout::top_down(Align::Min), |ui| {
                jb_tinted_card(ACCENT).jb_show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("🛡  Tus archivos están a salvo").font(jb_font(15.0)).color(ACCENT));
                    ui.label("En automático NUNCA se tocan videos, documentos, capturas ni descargas.");
                    ui.label(RichText::new("Si encuentra algo así, solo te avisa con una notificación para que tú decidas en la pestaña Limpieza.").size(13.0).color(MUTED));
                });
                ui.add_space(8.0);
                jb_card().jb_show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Condiciones inteligentes").font(jb_font(13.0)).color(ACCENT));
                    for line in [
                        "Enchufado o con más del 40 % de batería.",
                        "Sin juegos, videos ni presentaciones en pantalla completa.",
                        "Sin instalaciones ni actualizaciones en curso.",
                        "Prioridad baja de CPU y disco: no notarás nada.",
                    ] {
                        ui.label(RichText::new(format!("✔  {}", line)).size(13.5));
                    }
                    ui.label(RichText::new("Si algo no se cumple, espera y reintenta cada 10 minutos (hasta 2 horas).").size(12.5).color(MUTED));
                });
                ui.add_space(8.0);
                if simulating {
                    jb_card().jb_show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 120.0), Sense::hover());
                        jb_scanner(ui.painter(), r.center(), 46.0, t, ACCENT);
                        ui.vertical_centered(|ui| ui.label(RichText::new("Simulando limpieza automática…").color(ACCENT)));
                    });
                } else if let Some((freed, files, per)) = sim {
                    jb_tinted_card(ACCENT).jb_show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("Resultado de la simulación (no se borró nada)").font(jb_font(12.0)).color(ACCENT));
                        ui.label(RichText::new(format!("Liberaría {}", fmt::jb_bytes(*freed))).font(jb_font(26.0)));
                        ui.label(RichText::new(format!("{} archivos de basura · 0 videos o documentos", fmt::jb_count(*files))).color(MUTED));
                        ui.horizontal_wrapped(|ui| {
                            for (n, b) in per {
                                jb_badge(ui, &format!("{} · {}", n, fmt::jb_bytes(*b)), ACCENT);
                            }
                        });
                    });
                }
                if let Some(h) = &last_auto {
                    ui.add_space(8.0);
                    jb_card().jb_show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("Última ejecución automática").font(jb_font(12.0)).color(MUTED));
                        if h.mode == "pospuesta" {
                            ui.label(RichText::new(&h.note).color(WARN));
                        } else {
                            ui.label(RichText::new(format!("{} liberados · {}", fmt::jb_bytes(h.freed), fmt::jb_ago(h.ts))).font(jb_font(18.0)).color(ACCENT));
                        }
                        ui.label(RichText::new(fmt::jb_date(h.ts)).size(12.0).color(MUTED));
                    });
                }
                ui.add_space(8.0);
                jb_card().jb_show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(if cfg!(windows) { "Quitar NEXO de Windows" } else { "Quitar NEXO del sistema" }).font(jb_font(12.0)).color(MUTED));
                    ui.label(RichText::new("Borra la tarea automática, el vigía, el acceso directo y la copia instalada. Tu historial se conserva.").size(12.5).color(MUTED));
                    let label = if armed { "¿Seguro? Pulsa otra vez para quitarlo" } else { "Quitar NEXO del sistema" };
                    if jb_link(ui, label, if armed { DANGER } else { WARN }).clicked() {
                        uninstall_click = true;
                    }
                });
                });
            });
        });

        let now = ui.input(|i| i.time);
        if uninstall_click {
            if self.uninstall_armed {
                self.uninstall_armed = false;
                self.jb_show_toast("Quitando NEXO del sistema…".into(), ACCENT, now);
                self.jb_bg_action("Quitando NEXO del sistema", move |_progress| {
                    let done = install::jb_uninstall();
                    let reg = schedule::jb_is_registered();
                    Box::new(move |a: &mut JbNexoApp, now| {
                        a.task_registered = reg;
                        a.cfg.auto_enabled = false;
                        a.cfg.vigia_enabled = false;
                        a.cfg.jb_save();
                        a.vigia_running = false;
                        a.jb_show_toast(format!("NEXO quitado del sistema · {}", done.join(" · ")), WARN, now);
                    })
                });
            } else {
                self.uninstall_armed = true;
            }
        }
        if apply {
            self.cfg.jb_save();
            let (cfg, admin) = (self.cfg.clone(), self.admin);
            self.jb_show_toast("Aplicando…".into(), ACCENT, now);
            self.jb_bg_action("Programando la limpieza automática", move |_progress| {
                let res = if cfg.auto_enabled { schedule::jb_enable(&cfg, admin) } else { schedule::jb_disable() };
                let reg = schedule::jb_is_registered();
                Box::new(move |a: &mut JbNexoApp, now| {
                    a.task_registered = reg;
                    match res {
                        Ok(()) if cfg.auto_enabled => {
                            let msg = format!("Modo automático activado · {} a las {:02}:00", cfg.frequency.jb_label(), cfg.hour);
                            a.jb_show_toast(msg, ACCENT, now);
                        }
                        Ok(()) => a.jb_show_toast("Modo automático desactivado".into(), WARN, now),
                        Err(e) => a.jb_show_toast(format!("No se pudo programar la tarea: {}", e), DANGER, now),
                    }
                })
            });
        }
        if simulate {
            self.sim = None;
            let ids = self.cfg.auto_categories.clone();
            self.jb_spawn_scan(Some(ids), true);
        }
    }

    /// jimy-betanzos: Pantalla Historial de limpiezas.
    fn jb_page_history(&mut self, ui: &mut Ui) {
        jb_header(ui, "› Registro", "Historial de limpiezas", "Cada limpieza, manual o automática, queda anotada aquí.");
        if self.history.is_empty() {
            jb_card().jb_show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Aún no hay limpiezas registradas.").font(jb_font(18.0)));
                ui.label(RichText::new("Haz tu primer escaneo en la pestaña Limpieza.").color(MUTED));
            });
            return;
        }
        let total: u64 = self.history.iter().map(|h| h.freed).sum();
        ui.label(RichText::new(format!("Total liberado con NEXO: {}", fmt::jb_bytes(total))).font(jb_font(16.0)).color(ACCENT));
        ui.add_space(4.0);
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for h in self.history.iter().rev() {
                jb_card().jb_inner_margin(Margin::symmetric(18.0, 12.0)).jb_show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        match h.mode.as_str() {
                            "auto" => jb_badge(ui, "Automática", ACCENT),
                            "pospuesta" => jb_badge(ui, "Pospuesta", WARN),
                            _ => jb_badge(ui, "Manual", ACCENT),
                        }
                        ui.label(RichText::new(fmt::jb_date(h.ts)).font(jb_font(14.0)));
                        ui.label(RichText::new(fmt::jb_ago(h.ts)).size(12.0).color(MUTED));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if h.mode != "pospuesta" {
                                ui.label(RichText::new(fmt::jb_bytes(h.freed)).font(jb_font(22.0)).color(ACCENT));
                            }
                        });
                    });
                    if !h.note.is_empty() {
                        ui.label(RichText::new(&h.note).color(WARN));
                    }
                    if h.mode != "pospuesta" {
                        ui.label(RichText::new(format!(
                            "{} archivos · {} a la Papelera · {} en uso (intactos)",
                            fmt::jb_count(h.files),
                            fmt::jb_count(h.recycled),
                            fmt::jb_count(h.skipped)
                        )).size(12.5).color(MUTED));
                    }
                    if h.review_bytes > 0 {
                        ui.label(RichText::new(format!("Quedó para revisar: {} (capturas, descargas, videos y documentos)", fmt::jb_bytes(h.review_bytes))).size(12.5).color(ACCENT));
                    }
                    ui.horizontal_wrapped(|ui| {
                        for (n, b) in h.details.iter().filter(|(_, b)| *b > 0) {
                            jb_badge(ui, &format!("{} · {}", n, fmt::jb_bytes(*b)), ACCENT);
                        }
                    });
                });
                ui.add_space(4.0);
            }
        });
    }

    /// jimy-betanzos: Ventana para confirmar la limpieza.
    fn jb_confirm_modal(&mut self, ctx: &egui::Context) {
        let Some(results) = self.results.as_ref() else {
            self.confirm = false;
            return;
        };
        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("backdrop"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(200));
            });

        let chosen: Vec<&JbCatResult> = results.iter().filter(|r| r.jb_selected_size() > 0).collect();
        let (prot_n, prot_b) = chosen
            .iter()
            .filter(|r| r.kind == JbKind::Protected)
            .map(|r| (r.jb_selected_count(), r.jb_selected_size()))
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        let empties_bin = chosen.iter().any(|r| r.special == JbSpecial::RecycleBin);
        let total: u64 = chosen.iter().map(|r| r.jb_selected_size()).sum();
        let mut close = false;
        let mut go = false;
        let ack = &mut self.ack_protected;

        let area = egui::Area::new(egui::Id::new("confirm"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                jb_card().jb_fill(PANEL).jb_stroke(Stroke::new(1.5_f32, jb_alpha(ACCENT, 150))).jb_inner_margin(Margin::same(24.0)).jb_show(ui, |ui| {
                    ui.set_width(580.0);
                    ui.label(RichText::new("› Confirmar limpieza").font(jb_font(12.0)).color(ACCENT));
                    ui.label(RichText::new("Esto es lo que se va a hacer").font(jb_font(24.0)));
                    ui.add_space(6.0);
                    ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for r in &chosen {
                            let (col, dest) = match (r.kind, r.special) {
                                (_, JbSpecial::RecycleBin) => (WARN, "Se vacía la Papelera (definitivo)".to_string()),
                                (JbKind::Safe, _) => (ACCENT, "Eliminación definitiva · basura del sistema".to_string()),
                                (JbKind::Protected, _) => (ACCENT, format!("{} elementos → Papelera (recuperables)", r.jb_selected_count())),
                                _ => (WARN, format!("{} elementos → Papelera (recuperables)", r.jb_selected_count())),
                            };
                            ui.horizontal(|ui| {
                                let (d, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                                ui.painter().circle_filled(d.center(), 4.0, col);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(r.name).font(jb_font(15.0)));
                                    ui.label(RichText::new(dest).size(12.0).color(MUTED));
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(RichText::new(fmt::jb_bytes(r.jb_selected_size())).font(jb_font(15.0)).color(col));
                                });
                            });
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Total").font(jb_font(13.0)).color(MUTED));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(fmt::jb_bytes(total)).font(jb_font(22.0)).color(ACCENT));
                        });
                    });
                    if prot_n > 0 {
                        jb_tinted_card(ACCENT).jb_show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(format!("⚠  Incluye {} videos/documentos ({}).", prot_n, fmt::jb_bytes(prot_b))).color(ACCENT).font(jb_font(14.0)));
                            ui.label(RichText::new("Irán a la Papelera de reciclaje: podrás recuperarlos desde allí.").size(13.0));
                            ui.checkbox(ack, "Sí, quiero botar estos videos y documentos");
                        });
                    }
                    if empties_bin {
                        jb_tinted_card(WARN).jb_show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new("⚠  Vaciar la Papelera borra su contenido para siempre.").color(WARN));
                        });
                    }
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if jb_button(ui, "✔  Confirmar", ACCENT, vec2(190.0, 44.0), prot_n == 0 || *ack).clicked() {
                            go = true;
                        }
                        if jb_button(ui, "Cancelar", MUTED, vec2(150.0, 44.0), true).clicked() {
                            close = true;
                        }
                    });
                });
            });
        ctx.move_to_top(area.response.layer_id);
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if go {
            self.confirm = false;
            self.ack_protected = false;
            self.jb_spawn_clean();
        } else if close {
            self.confirm = false;
        }
    }

    // diagnóstico y protección (fase 3)

    /// jimy-betanzos: Recibe los resultados del diagnóstico y del antivirus.
    fn jb_poll_phase3(&mut self, now: f64) {
        if let Some(r) = self.diag_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.diag_rx = None;
            self.trim_in = 3;
            match r {
                Ok(rep) => {
                    let s = rep.overall;
                    self.diag = Some(rep);
                    self.jb_show_toast(format!("Diagnóstico listo · {}/100 · {}", s, diag::jb_grade(s)), jb_score_color(s), now);
                }
                Err(e) => self.jb_show_toast(format!("No se pudo completar el diagnóstico: {}", e), DANGER, now),
            }
        }
        if let Some((r, secs)) = self.mpscan_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.mpscan_rx = None;
            if r.is_ok() && secs > 5 {
                // Se recuerda cuánto tardó para estimar mejor la próxima barra.
                self.cfg.defender_scan_secs = secs;
                self.cfg.jb_save();
            }
            let (msg, col) = match r {
                Ok(0) => ("✔ Análisis rápido terminado: no se encontraron amenazas.".to_string(), ACCENT),
                Ok(2) => (format!("⚠ {} encontró amenazas. Revisa {}.", diag::jb_txt("av"), diag::jb_txt("security_app")), DANGER),
                Ok(c) => (format!("El análisis terminó con el código {}. Revisa {}.", c, diag::jb_txt("security_app")), WARN),
                Err(e) => (format!("No se pudo lanzar el análisis: {}", e), DANGER),
            };
            notify::jb_toast(&format!("Análisis de {}", diag::jb_txt("av")), &msg, "");
            if col == ACCENT {
                if let Some(d) = self.diag.as_mut() {
                    d.raw.defender.quick_age = 0;
                    diag::jb_analyze(d);
                    diag::jb_save(d);
                }
            }
            self.mpscan_msg = Some((msg, col));
        }
    }

    /// jimy-betanzos: Lanza el diagnóstico en un hilo aparte.
    fn jb_start_diag(&mut self) {
        if self.diag_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        self.diag_rx = Some(rx);
        self.diag_started = self.now;
        let prog = self.diag_prog.clone();
        prog.store(0, std::sync::atomic::Ordering::Relaxed);
        std::thread::spawn(move || {
            sys::jb_interactive_thread();
            let _ = tx.send(diag::jb_run_with_progress(&prog));
        });
    }

    /// jimy-betanzos: Lanza el análisis rápido del antivirus.
    fn jb_start_mpscan(&mut self) {
        if self.mpscan_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        self.mpscan_rx = Some(rx);
        self.mpscan_msg = None;
        self.mp_started = self.now;
        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            let r = diag::jb_quick_scan();
            let _ = tx.send((r, start.elapsed().as_secs()));
        });
    }

    /// jimy-betanzos: Ejecuta la acción de una recomendación.
    fn jb_do_action(&mut self, a: JbAction, now: f64) {
        match a {
            JbAction::GoClean => self.page = JbPage::Clean,
            JbAction::GoRam => self.page = JbPage::Ram,
            JbAction::QuickScan => {
                self.jb_start_mpscan();
                self.page = JbPage::Shield;
            }
            JbAction::SetBalanced => self.jb_set_balanced(true, now),
            JbAction::BatteryReport => {
                std::thread::spawn(diag::jb_battery_report);
                self.jb_show_toast("Abriendo el informe de batería…".into(), ACCENT, now);
            }
            JbAction::Open(t) => diag::jb_open(&t),
        }
    }

    /// jimy-betanzos: Cambia a «Equilibrado» guardando el plan anterior, o lo restaura (deshacer).
    fn jb_set_balanced(&mut self, on: bool, now: f64) {
        if self.pending_actions > 0 {
            return;
        }
        let prev = self.cfg.prev_power_plan.clone();
        if !on && prev.is_none() {
            return;
        }
        self.jb_show_toast("Cambiando el plan de energía…".into(), ACCENT, now);
        self.jb_bg_action("Cambiando el plan de energía", move |_progress| {
            let (cur, _) = diag::jb_active_power_plan();
            let target = if on { diag::BALANCED.to_string() } else { prev.unwrap_or_default() };
            let ok = diag::jb_set_power_plan(&target);
            let (g, n) = diag::jb_active_power_plan();
            Box::new(move |a: &mut JbNexoApp, now| {
                if !ok {
                    a.jb_show_toast("No se pudo cambiar el plan de energía.".into(), DANGER, now);
                    return;
                }
                if on && cur != diag::BALANCED {
                    a.cfg.prev_power_plan = Some(cur);
                }
                if !on {
                    a.cfg.prev_power_plan = None;
                }
                a.cfg.jb_save();
                if let Some(d) = a.diag.as_mut() {
                    d.power_guid = g;
                    d.power_name = n.clone();
                    diag::jb_analyze(d);
                    diag::jb_save(d);
                }
                a.jb_show_toast(format!("Plan de energía: {}", n), ACCENT, now);
            })
        });
    }

    /// jimy-betanzos: Activa o desactiva un programa de inicio.
    fn jb_toggle_startup(&mut self, i: usize, on: bool, now: f64) {
        let result = {
            let Some(d) = self.diag.as_mut() else { return };
            let Some(item) = d.raw.startup.get(i).cloned() else { return };
            let r = diag::jb_set_startup_enabled(&item, on);
            if r.is_ok() {
                d.raw.startup[i].enabled = on;
                diag::jb_analyze(d);
                diag::jb_save(d);
            }
            r.map(|_| item.name)
        };
        match result {
            Ok(name) => self.jb_show_toast(
                format!("{} {} al encender {}", name, if on { "se abrirá" } else { "ya no se abrirá" }, diag::jb_txt("os")),
                ACCENT,
                now,
            ),
            Err(e) => self.jb_show_toast(e, DANGER, now),
        }
    }

    /// jimy-betanzos: Vista mientras corre el diagnóstico.
    fn jb_diag_running_view(&mut self, ui: &mut Ui, t: f32) {
        jb_card().jb_show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 280.0), Sense::hover());
            jb_scanner(ui.painter(), r.center(), 104.0, t, ACCENT);
            ui.vertical_centered(|ui| {
                let f = self.diag_prog.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
                ui.label(RichText::new("Analizando tu equipo…").font(jb_font(24.0)).color(ACCENT));
                ui.label(RichText::new(format!("{:.0} %", f * 100.0)).font(jb_font(30.0)).color(TEXT));
                ui.allocate_ui(vec2(460.0, 12.0), |ui| jb_progress_bar(ui, Some(f), ACCENT, t));
                ui.label(RichText::new("Disco · batería · antivirus · firewall · actualizaciones · programas de inicio · tareas programadas").size(13.0));
                ui.label(RichText::new(format!("{} transcurridos · solo lee: no cambia nada", fmt::jb_secs((self.now - self.diag_started).max(0.0)))).size(12.5).color(MUTED));
            });
        });
    }

    /// jimy-betanzos: Tarjeta inicial del diagnóstico con su botón.
    fn jb_diag_hero(&mut self, ui: &mut Ui, title: &str, sub: &str, action: &str) {
        let mut go = false;
        jb_card().jb_show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 240.0), Sense::hover());
            jb_scanner(ui.painter(), r.center(), 92.0, 0.8, jb_alpha(ACCENT, 170));
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(title).font(jb_font(22.0)));
                ui.label(RichText::new(sub).size(13.5).color(MUTED));
                ui.add_space(8.0);
                if jb_button(ui, action, ACCENT, vec2(320.0, 50.0), true).clicked() {
                    go = true;
                }
            });
        });
        if go {
            self.jb_start_diag();
        }
    }

    /// jimy-betanzos: Pantalla Diagnóstico.
    fn jb_page_diag(&mut self, ui: &mut Ui, t: f32) {
        jb_header(ui, "› Diagnóstico", "Chequeo general", "Una nota de 0 a 100 por áreas, calculada aquí mismo y sin internet. Las mejoras se pueden deshacer.");
        let now = ui.input(|i| i.time);
        if self.diag_rx.is_some() {
            self.jb_diag_running_view(ui, t);
            return;
        }
        let Some(rep) = self.diag.clone() else {
            self.jb_diag_hero(ui, "Revisa la salud de tu equipo", "Disco, batería, memoria, seguridad y programas de inicio. Solo lee: no cambia nada.", "🔍  Iniciar diagnóstico");
            return;
        };
        let admin = self.admin;
        let can_undo = self.cfg.prev_power_plan.is_some();
        let mut action: Option<JbAction> = None;
        let mut toggle_item: Option<(usize, bool)> = None;
        let mut rerun = false;
        let mut undo_power = false;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |c| {
                jb_card().jb_show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(200.0, 196.0), Sense::hover());
                        let col = jb_score_color(rep.overall);
                        jb_ring(ui.painter(), r.center(), 72.0, rep.overall as f32 / 100.0, col, 10.0);
                        ui.painter().text(r.center() - vec2(0.0, 8.0), Align2::CENTER_CENTER, rep.overall.to_string(), jb_font(46.0), TEXT);
                        ui.painter().text(r.center() + vec2(0.0, 26.0), Align2::CENTER_CENTER, "DE 100", jb_font(11.0), col);
                        ui.vertical(|ui| {
                            ui.add_space(24.0);
                            ui.label(RichText::new(diag::jb_grade(rep.overall)).font(jb_font(28.0)).color(col));
                            ui.label(RichText::new(format!("Diagnóstico {}", fmt::jb_ago(rep.ts))).size(12.5).color(MUTED));
                            let n2 = rep.recs.iter().filter(|r| r.severity == 2).count();
                            let n1 = rep.recs.iter().filter(|r| r.severity == 1).count();
                            ui.label(RichText::new(format!("{} importantes · {} para revisar", n2, n1)).size(13.0));
                            ui.add_space(8.0);
                            if jb_button(ui, "⟳  Repetir", ACCENT, vec2(160.0, 40.0), true).clicked() {
                                rerun = true;
                            }
                        });
                    });
                });
                jb_card().jb_show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Notas por área").font(jb_font(12.0)).color(MUTED));
                    let w = ui.available_width() / rep.areas.len().max(1) as f32;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (name, score) in &rep.areas {
                            let (r, _) = ui.allocate_exact_size(vec2(w, 170.0), Sense::hover());
                            let c0 = r.center() - vec2(0.0, 12.0);
                            let p = ui.painter();
                            match score {
                                Some(s) => {
                                    let col = jb_score_color(*s);
                                    jb_ring(p, c0, 26.0, *s as f32 / 100.0, col, 5.0);
                                    p.text(c0, Align2::CENTER_CENTER, s.to_string(), jb_font(20.0), TEXT);
                                }
                                None => {
                                    jb_ring(p, c0, 26.0, 0.0, MUTED, 5.0);
                                    p.text(c0, Align2::CENTER_CENTER, "—", jb_font(20.0), MUTED);
                                }
                            }
                            let label = if name == "Almacenamiento" { "Disco" } else { name.as_str() };
                            p.text(pos2(r.center().x, r.bottom() - 22.0), Align2::CENTER_CENTER, label, jb_font(12.5), TEXT);
                        }
                    });
                });
            });

            jb_section(ui, "Recomendaciones", "ordenadas por importancia", ACCENT);
            if rep.recs.is_empty() {
                jb_tinted_card(ACCENT).jb_show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("✔ Todo en orden: no hay nada que mejorar ahora mismo.").color(ACCENT));
                });
            }
            for r in &rep.recs {
                let (col, tag) = match r.severity {
                    2 => (DANGER, "Importante"),
                    1 => (WARN, "Revisar"),
                    _ => (ACCENT, "Consejo"),
                };
                jb_card().jb_stroke(Stroke::new(1.0_f32, jb_alpha(col, 90))).jb_inner_margin(Margin::symmetric(16.0, 10.0)).jb_show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() - 190.0).max(260.0);
                        ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::LEFT), |ui| {
                            ui.horizontal(|ui| {
                                jb_badge(ui, tag, col);
                                jb_badge(ui, &r.area, MUTED);
                                ui.label(RichText::new(&r.title).font(jb_font(15.0)));
                            });
                            ui.label(RichText::new(&r.detail).size(12.5).color(MUTED));
                        });
                        if let Some(a) = &r.action {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if jb_button(ui, a.jb_label(), col, vec2(170.0, 36.0), true).clicked() {
                                    action = Some(a.clone());
                                }
                            });
                        }
                    });
                });
                ui.add_space(2.0);
            }

            jb_section(ui, "Programas de inicio", "desactivarlos no los desinstala · se pueden volver a activar", ACCENT);
            jb_card().jb_inner_margin(Margin::symmetric(16.0, 10.0)).jb_show(ui, |ui| {
                ui.set_width(ui.available_width());
                let on_count = rep.raw.startup.iter().filter(|s| s.enabled).count();
                ui.label(RichText::new(format!("{} de {} se abren al encender {}", on_count, rep.raw.startup.len(), diag::jb_txt("os"))).size(12.5).color(MUTED));
                for (i, s) in rep.raw.startup.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let locked = s.jb_machine_wide() && !admin;
                        let mut on = s.enabled;
                        if ui.add_enabled_ui(!locked, |ui| jb_toggle(ui, &mut on)).inner.changed() {
                            toggle_item = Some((i, on));
                        }
                        let name: String = if s.name.chars().count() > 42 { s.name.chars().take(40).collect::<String>() + "…" } else { s.name.clone() };
                        ui.label(RichText::new(name).font(jb_font(14.5)).color(if s.enabled { TEXT } else { MUTED })).on_hover_text(&s.command);
                        if s.suspicious {
                            jb_badge(ui, "Sospechoso", DANGER);
                        } else if s.jb_missing() {
                            jb_badge(ui, "Roto", WARN);
                        } else if !s.jb_signed() && !s.jb_is_nexo() {
                            jb_badge(ui, "Sin firma", WARN);
                        }
                        if s.jb_machine_wide() {
                            jb_badge(ui, "Todo el equipo", ACCENT);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let who = if !s.publisher.is_empty() {
                                s.publisher.clone()
                            } else if s.jb_missing() {
                                "programa no encontrado".to_string()
                            } else {
                                "editor desconocido".to_string()
                            };
                            ui.label(RichText::new(who).size(12.0).color(MUTED));
                        });
                    });
                }
                if !admin && rep.raw.startup.iter().any(|s| s.jb_machine_wide()) {
                    ui.label(RichText::new("Los de «todo el equipo» se cambian abriendo NEXO como administrador.").size(12.0).color(WARN));
                }
            });

            jb_section(ui, "Detalles", "datos leídos de tu equipo", ACCENT);
            ui.columns(3, |c| {
                jb_card().jb_show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Disco").font(jb_font(12.0)).color(MUTED));
                    for d in &rep.raw.disks {
                        ui.label(RichText::new(&d.name).font(jb_font(14.0)));
                        let health = if d.health == "Healthy" { "buena".to_string() } else { d.health.clone() };
                        ui.label(RichText::new(format!("{} · {} · salud {}", d.media, fmt::jb_bytes(d.size.max(0) as u64), health)).size(12.5));
                        match (d.wear, d.temp) {
                            (Some(w), Some(tc)) => ui.label(RichText::new(format!("Desgaste {} % · {} °C", w, tc)).size(12.5)),
                            _ => ui.label(RichText::new("Desgaste y temperatura: abre NEXO como administrador").size(12.0).color(MUTED)),
                        };
                    }
                    ui.label(RichText::new(format!("Libre: {} de {}", fmt::jb_bytes(rep.disk_free), fmt::jb_bytes(rep.disk_total))).size(12.5));
                });
                jb_card().jb_show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Batería").font(jb_font(12.0)).color(MUTED));
                    let b = &rep.raw.battery;
                    match b.jb_health() {
                        Some(h) => {
                            let col = jb_score_color(h as u8);
                            ui.label(RichText::new(format!("{:.0} % de salud", h)).font(jb_font(22.0)).color(col));
                            jb_bar(ui, h / 100.0, col, 6.0);
                            ui.label(RichText::new(format!("Capacidad {:.1} Wh de {:.1} Wh originales", b.full as f32 / 1000.0, b.design as f32 / 1000.0)).size(12.5));
                            ui.label(RichText::new(format!("{} ciclos de carga", b.cycles)).size(12.5));
                        }
                        None => {
                            ui.label(RichText::new("No se detectó batería.").color(MUTED));
                        }
                    }
                    if jb_link(ui, "Ver el informe completo del sistema", ACCENT).clicked() {
                        action = Some(JbAction::BatteryReport);
                    }
                });
                jb_card().jb_show(&mut c[2], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Sistema").font(jb_font(12.0)).color(MUTED));
                    ui.label(RichText::new(&rep.raw.os).size(12.5));
                    ui.label(RichText::new(format!("Encendido desde hace {:.1} días", rep.raw.uptime_days)).size(12.5));
                    let upd = if rep.raw.last_update_days >= 0 { format!("Última actualización hace {} días", rep.raw.last_update_days) } else { "Última actualización: desconocida".into() };
                    ui.label(RichText::new(upd).size(12.5));
                    ui.label(RichText::new(format!("Plan de energía: {}", rep.power_name)).size(12.5));
                    if rep.power_guid != diag::BALANCED && jb_link(ui, "Usar «Equilibrado»", ACCENT).clicked() {
                        action = Some(JbAction::SetBalanced);
                    }
                    if can_undo && jb_link(ui, "Deshacer el cambio de plan", WARN).clicked() {
                        undo_power = true;
                    }
                });
            });
        });

        if rerun {
            self.jb_start_diag();
        }
        if let Some(a) = action {
            self.jb_do_action(a, now);
        }
        if undo_power {
            self.jb_set_balanced(false, now);
        }
        if let Some((i, on)) = toggle_item {
            self.jb_toggle_startup(i, on, now);
        }
    }

    /// jimy-betanzos: Pantalla Protección.
    fn jb_page_shield(&mut self, ui: &mut Ui, t: f32) {
        jb_header(ui, "› Escudo", "Protección", &format!("Revisa la seguridad de {} sin internet, apoyándose en {}.", diag::jb_txt("os"), diag::jb_txt("av")));
        let now = ui.input(|i| i.time);
        if self.diag_rx.is_some() {
            self.jb_diag_running_view(ui, t);
            return;
        }
        let Some(rep) = self.diag.clone() else {
            self.jb_diag_hero(ui, "Revisa la protección de tu equipo", "Antivirus, firewall, actualizaciones, programas de inicio y tareas sospechosas.", "🔒  Revisar protección");
            return;
        };
        let sec = rep.areas.iter().find(|(a, _)| a == "Seguridad").and_then(|(_, s)| *s).unwrap_or(0);
        let scanning = self.mpscan_rx.is_some();
        let mp_started = self.mp_started;
        let mp_est = if self.cfg.defender_scan_secs > 0 { self.cfg.defender_scan_secs as f64 } else { 240.0 };
        let scan_msg = self.mpscan_msg.clone();
        let vigia_running = self.vigia_running || vigia::jb_is_running();
        let mut action: Option<JbAction> = None;
        let mut scan = false;
        let mut rerun = false;
        let mut cfg_changed = false;
        let cfg = &mut self.cfg;
        let r = &rep.raw;
        let d = &r.defender;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |c| {
                jb_card().jb_show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (rr, _) = ui.allocate_exact_size(vec2(190.0, 186.0), Sense::hover());
                        let col = jb_score_color(sec);
                        jb_ring(ui.painter(), rr.center(), 68.0, sec as f32 / 100.0, col, 10.0);
                        ui.painter().text(rr.center() - vec2(0.0, 6.0), Align2::CENTER_CENTER, "🛡", egui::FontId::proportional(40.0), col);
                        ui.painter().text(rr.center() + vec2(0.0, 30.0), Align2::CENTER_CENTER, format!("{}/100", sec), jb_font(13.0), col);
                        ui.vertical(|ui| {
                            ui.add_space(26.0);
                            let (txt, col) = if sec >= 85 { ("Protegido", ACCENT) } else if sec >= 60 { ("Revisar", WARN) } else { ("En riesgo", DANGER) };
                            ui.label(RichText::new(txt).font(jb_font(30.0)).color(col));
                            ui.label(RichText::new(format!("Revisado {}", fmt::jb_ago(rep.ts))).size(12.5).color(MUTED));
                            ui.add_space(8.0);
                            if jb_link(ui, "⟳ Volver a revisar", ACCENT).clicked() {
                                rerun = true;
                            }
                        });
                    });
                });
                jb_card().jb_show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(diag::jb_txt("av")).font(jb_font(13.0)).color(ACCENT));
                    if d.available {
                        let q = if d.quick_age >= 0 && d.quick_age < 10000 { format!("hace {} días", d.quick_age) } else { "nunca".into() };
                        ui.label(RichText::new(format!("Último análisis rápido: {}", q)).size(13.0));
                        ui.label(RichText::new(format!("Firmas actualizadas hace {} días · se actualizan con {}", d.sig_age, diag::jb_txt("updater"))).size(12.5).color(MUTED));
                    } else {
                        ui.label(RichText::new(diag::jb_txt("av_missing_title")).color(WARN));
                    }
                    ui.add_space(6.0);
                    if scanning {
                        let f = ((now - mp_started) / mp_est).min(0.95) as f32;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Analizando con {}…", diag::jb_txt("av"))).color(ACCENT).size(13.0));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(format!("{:.0} %", f * 100.0)).font(jb_font(15.0)).color(ACCENT));
                            });
                        });
                        jb_progress_bar(ui, Some(f), ACCENT, t);
                        ui.label(RichText::new(format!("{} de ≈ {} (estimado; el sistema no informa el avance real). Puedes seguir usando el equipo.", fmt::jb_secs(now - mp_started), fmt::jb_secs(mp_est))).size(11.5).color(MUTED));
                    } else {
                        let w = ui.available_width();
                        if diag::jb_quick_scan_available() && jb_button(ui, "🛡  Análisis rápido", ACCENT, vec2(w, 44.0), true).clicked() {
                            scan = true;
                        }
                    }
                    if let Some((m, col)) = &scan_msg {
                        ui.label(RichText::new(m).color(*col).size(13.0));
                    }
                });
            });

            jb_section(ui, "Comprobaciones", &format!("lectura local de la configuración de {}", diag::jb_txt("os")), ACCENT);
            jb_card().jb_inner_margin(Margin::symmetric(16.0, 8.0)).jb_show(ui, |ui| {
                ui.set_width(ui.available_width());
                let fw: Vec<String> = r.firewall.iter().map(|f| format!("{} {}", match f.name.as_str() { "Domain" => "Dominio", "Private" => "Privada", "Public" => "Pública", n => n }, if f.enabled { "✔" } else { "✖" })).collect();
                let susp = r.startup.iter().filter(|s| s.enabled && s.suspicious).count();
                let unsigned = r.startup.iter().filter(|s| s.enabled && !s.jb_signed() && !s.jb_missing() && !s.jb_is_nexo()).count();
                let open = |k: &str| diag::jb_setting(k).map(|t| JbAction::Open(t.to_string()));
                let rows: Vec<(u8, String, String, Option<JbAction>)> = vec![
                    (
                        if d.available && d.realtime && d.enabled { 0 } else { 2 },
                        "Antivirus en tiempo real".into(),
                        if !d.available { diag::jb_txt("av").to_string() + " no está disponible" } else if d.realtime { format!("{} te protege mientras usas el equipo", diag::jb_txt("av")) } else { "Desactivado: actívalo cuanto antes".into() },
                        open("realtime"),
                    ),
                    (if d.sig_age <= 7 { 0 } else { 1 }, "Firmas del antivirus".into(), format!("Actualizadas hace {} días", d.sig_age), open("updates")),
                    (if r.firewall.iter().all(|f| f.enabled) && !r.firewall.is_empty() { 0 } else { 2 }, "Firewall".into(), fw.join(" · "), open("firewall")),
                    (if r.uac == Some(false) { 2 } else if r.uac.is_none() { 3 } else { 0 }, diag::jb_txt("uac_name").into(), match r.uac { Some(false) => "Desactivado".into(), None => "No aplica en este sistema".into(), _ => diag::jb_txt("uac_on").into() }, None),
                    (
                        if (0..=45).contains(&r.last_update_days) { 0 } else if r.last_update_days < 0 { 3 } else { 1 },
                        format!("Actualizaciones de {}", diag::jb_txt("os")),
                        if r.last_update_days >= 0 { format!("Última instalada hace {} días", r.last_update_days) } else { "No se pudo leer".into() },
                        open("updates"),
                    ),
                    (if r.rdp { 1 } else { 0 }, diag::jb_txt("remote_name").into(), if r.rdp { "Activado: desactívalo si no lo usas".into() } else { "Desactivado".into() }, if r.rdp { open("remote") } else { None }),
                    (if r.proxy.is_empty() { 0 } else { 1 }, "Proxy de red".into(), if r.proxy.is_empty() { "Sin proxy: conexión directa".into() } else { format!("Configurado: {}", r.proxy) }, if r.proxy.is_empty() { None } else { open("proxy") }),
                    {
                        let unknown = diag::jb_unknown_hosts(&r.hosts);
                        (
                            if unknown.is_empty() { 0 } else { 1 },
                            "Archivo hosts".into(),
                            if r.hosts.is_empty() {
                                "Sin cambios".into()
                            } else if unknown.is_empty() {
                                format!("{} entradas, todas de programas conocidos o del propio equipo", r.hosts.len())
                            } else {
                                format!("{} entradas desconocidas: pueden redirigir páginas", unknown.len())
                            },
                            None,
                        )
                    },
                    (
                        match r.encryption { 1 => 0, 2 => 1, _ => 3 },
                        format!("Cifrado del disco ({})", diag::jb_txt("encryption")),
                        match r.encryption { 1 => "Activado".into(), 2 => "Desactivado".into(), _ => "No disponible en esta edición o equipo".into() },
                        if r.encryption == 2 { open("encryption") } else { None },
                    ),
                    (if susp > 0 { 2 } else if unsigned > 0 { 1 } else { 0 }, "Programas de inicio".into(), format!("{} sospechosos · {} sin firma digital (activos)", susp, unsigned), None),
                    (if r.tasks.is_empty() { 0 } else { 2 }, "Tareas programadas".into(), format!("{} de terceros revisadas · {} sospechosas", r.tasks_total, r.tasks.len()), if r.tasks.is_empty() { None } else { open("tasks") }),
                ];
                for (state, title, detail, act) in rows {
                    if jb_check_row(ui, state, &title, &detail, act.as_ref()) {
                        action = act;
                    }
                }
            });

            if !r.tasks.is_empty() || r.startup.iter().any(|s| s.enabled && (s.suspicious || (!s.jb_signed() && !s.jb_missing() && !s.jb_is_nexo()))) {
                jb_section(ui, "Para revisar", "si no reconoces algo, desactívalo en Diagnóstico › Programas de inicio", WARN);
                jb_card().jb_show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for s in r.startup.iter().filter(|s| s.enabled && (s.suspicious || (!s.jb_signed() && !s.jb_missing() && !s.jb_is_nexo()))) {
                        ui.label(RichText::new(format!("{} · {}", s.name, if s.suspicious { "carpeta poco habitual" } else { "sin firma digital" })).font(jb_font(14.0)));
                        ui.label(RichText::new(&s.exe).monospace().size(11.5).color(MUTED));
                    }
                    for tk in &r.tasks {
                        ui.label(RichText::new(format!("Tarea «{}» · {}", tk.name, tk.why)).font(jb_font(14.0)).color(DANGER));
                        ui.label(RichText::new(&tk.exe).monospace().size(11.5).color(MUTED));
                    }
                });
            }

            jb_section(ui, "Vigilancia continua", "el vigía revisa esto cada 30 segundos", ACCENT);
            jb_card().jb_show(ui, |ui| {
                ui.set_width(ui.available_width());
                cfg_changed |= ui.checkbox(&mut cfg.alert_security, format!("Avisar si {} se apaga o si un programa nuevo se añade al inicio de {}", diag::jb_txt("av"), diag::jb_txt("os"))).changed();
                let (txt, col) = if vigia_running {
                    ("✔ El vigía está en marcha.", ACCENT)
                } else {
                    ("El vigía está detenido: actívalo en Memoria RAM para recibir estos avisos.", WARN)
                };
                ui.label(RichText::new(txt).size(12.5).color(col));
            });
        });

        if cfg_changed {
            self.cfg_dirty = Some(now);
        }
        if rerun {
            self.jb_start_diag();
        }
        if scan {
            self.jb_start_mpscan();
        }
        if let Some(a) = action {
            self.jb_do_action(a, now);
        }
    }

    // celulares (fase 4)

    /// jimy-betanzos: Lanza una tarea con celulares en un hilo aparte.
    fn jb_adb_spawn(&mut self, label: &str, f: impl FnOnce() -> JbAdbMsg + Send + 'static) {
        if self.adb_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        self.adb_rx = Some(rx);
        self.adb_busy = Some(label.to_string());
        self.adb_started = self.now;
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
    }

    /// jimy-betanzos: Vuelve a buscar celulares conectados.
    fn jb_adb_refresh(&mut self) {
        self.jb_adb_spawn("Buscando celulares…", || JbAdbMsg::Devices(adb::jb_devices()));
    }

    /// jimy-betanzos: Lee los datos de un celular.
    fn jb_adb_load(&mut self, serial: String) {
        // Al recargar el mismo celular se conserva lo mostrado: la página no salta arriba.
        if self.dev_sel.as_deref() != Some(serial.as_str()) {
            self.dev_info = None;
        }
        self.dev_sel = Some(serial.clone());
        self.jb_adb_spawn("Leyendo el celular…", move || {
            let info = adb::jb_info(&serial);
            let bloat = adb::jb_bloat(&serial).unwrap_or_default();
            let usage = adb::jb_battery_usage(&serial).unwrap_or_default();
            JbAdbMsg::Loaded(serial, info, bloat, usage)
        });
    }

    /// jimy-betanzos: Recibe los resultados de las tareas con celulares.
    fn jb_poll_adb(&mut self, now: f64) {
        let Some(msg) = self.adb_rx.as_ref().and_then(|rx| rx.try_recv().ok()) else { return };
        self.adb_rx = None;
        self.adb_busy = None;
        match msg {
            JbAdbMsg::Devices(Ok(d)) => {
                self.devices = d;
                let keep = self.dev_sel.as_ref().is_some_and(|s| self.devices.iter().any(|d| &d.serial == s && d.jb_ready()));
                if !keep {
                    self.dev_info = None;
                    self.dev_sel = None;
                    if let Some(first) = self.devices.iter().find(|d| d.jb_ready()).map(|d| d.serial.clone()) {
                        self.jb_adb_load(first);
                    }
                }
            }
            JbAdbMsg::Devices(Err(e)) => self.adb_msg = Some((format!("ADB: {}", e), DANGER)),
            JbAdbMsg::Loaded(serial, info, bloat, usage) => {
                if self.dev_sel.as_deref() == Some(serial.as_str()) {
                    match info {
                        Ok(i) => self.dev_info = Some(i),
                        Err(e) => self.adb_msg = Some((format!("No se pudo leer el celular: {}", e), DANGER)),
                    }
                    self.dev_bloat = bloat;
                    self.dev_usage = usage;
                }
            }
            JbAdbMsg::Done(res, rescan) => {
                match res {
                    Ok(m) => {
                        self.jb_show_toast(m.clone(), ACCENT, now);
                        self.adb_msg = Some((m, ACCENT));
                    }
                    Err(e) => self.adb_msg = Some((e, DANGER)),
                }
                if rescan {
                    self.jb_adb_refresh();
                } else if let Some(s) = self.dev_sel.clone() {
                    self.jb_adb_load(s);
                }
            }
        }
    }

    /// jimy-betanzos: Pantalla Celulares.
    fn jb_page_mobile(&mut self, ui: &mut Ui, t: f32) {
        jb_header(ui, "› Enlace móvil", "Celulares", "Realme C11 2021 y Honor X7 · por USB o WiFi local, sin internet.");
        if adb::jb_adb_path().is_none() {
            jb_tinted_card(WARN).jb_show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("No se encontró ADB (Android platform-tools).").color(WARN).font(jb_font(15.0)));
                ui.label("Copia la carpeta «platform-tools» junto a NEXO para usar el modo potenciado.");
            });
            return;
        }
        if !self.adb_scanned {
            self.adb_scanned = true;
            self.jb_adb_refresh();
        }
        /// jimy-betanzos: Acción pedida sobre un celular.
        enum JbAct {
            Refresh,
            Select(String),
            Pair,
            Connect,
            Trim,
            Install,
            Toggle(String, bool),
        }
        let mut act: Option<JbAct> = None;
        let busy = self.adb_busy.clone();
        let msg = self.adb_msg.clone();
        let apk = adb::jb_mobile_apk();
        let sel = self.dev_sel.clone();
        let devices = self.devices.clone();
        let info = self.dev_info.clone();
        let (pair_addr, pair_code, conn_addr) = (&mut self.pair_addr, &mut self.pair_code, &mut self.conn_addr);
        let bloat = &self.dev_bloat;
        let usage = &self.dev_usage;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |c| {
                jb_card().jb_show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Tus celulares").font(jb_font(13.0)).color(ACCENT));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if busy.is_none() && jb_link(ui, "⟳ Buscar", ACCENT).clicked() {
                                act = Some(JbAct::Refresh);
                            }
                        });
                    });
                    if devices.is_empty() {
                        ui.label(RichText::new("Ningún celular conectado. Conéctalo por USB o por WiFi.").color(MUTED));
                    }
                    for d in &devices {
                        let selected = sel.as_deref() == Some(d.serial.as_str());
                        let frame = if selected { jb_tinted_card(ACCENT) } else { jb_card() };
                        let r = frame.jb_inner_margin(Margin::symmetric(12.0, 8.0)).jb_show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("📱").size(18.0));
                                ui.label(RichText::new(if d.model.is_empty() { d.serial.clone() } else { d.model.clone() }).font(jb_font(15.0)));
                                jb_badge(ui, if d.jb_wifi() { "WiFi" } else { "USB" }, ACCENT);
                                match d.state.as_str() {
                                    "device" => jb_badge(ui, "Listo", ACCENT),
                                    "unauthorized" => jb_badge(ui, "Acepta el aviso en el celular", WARN),
                                    s => jb_badge(ui, s, MUTED),
                                }
                            });
                        });
                        if d.jb_ready() && !selected && r.response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            act = Some(JbAct::Select(d.serial.clone()));
                        }
                    }
                    if let Some(b) = &busy {
                        let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 70.0), Sense::hover());
                        jb_scanner(ui.painter(), rr.center(), 24.0, t, ACCENT);
                        ui.vertical_centered(|ui| ui.label(RichText::new(b).color(ACCENT)));
                    }
                    if let Some((m, col)) = &msg {
                        ui.label(RichText::new(m).color(*col).size(13.0));
                    }
                });
                jb_card().jb_show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Conectar por WiFi (Android 11)").font(jb_font(13.0)).color(ACCENT));
                    ui.label(RichText::new("En el celular: Opciones de desarrollador › Depuración inalámbrica › «Vincular con código».").size(12.5).color(MUTED));
                    ui.add(egui::TextEdit::singleline(pair_addr).hint_text(RichText::new("IP:puerto de vinculación  (ej. 192.168.1.40:37123)").color(jb_alpha(MUTED, 160))).desired_width(f32::INFINITY));
                    ui.add(egui::TextEdit::singleline(pair_code).hint_text(RichText::new("Código de 6 dígitos").color(jb_alpha(MUTED, 160))).desired_width(f32::INFINITY));
                    if jb_button(ui, "Vincular", ACCENT, vec2(ui.available_width(), 36.0), busy.is_none() && !pair_addr.trim().is_empty() && !pair_code.trim().is_empty()).clicked() {
                        act = Some(JbAct::Pair);
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new("Después, con la IP y el puerto de «Depuración inalámbrica» (no los de vincular):").size(12.5).color(MUTED));
                    ui.add(egui::TextEdit::singleline(conn_addr).hint_text(RichText::new("IP:puerto  (ej. 192.168.1.40:41235)").color(jb_alpha(MUTED, 160))).desired_width(f32::INFINITY));
                    if jb_button(ui, "Conectar", ACCENT, vec2(ui.available_width(), 36.0), busy.is_none() && !conn_addr.trim().is_empty()).clicked() {
                        act = Some(JbAct::Connect);
                    }
                    ui.label(RichText::new("Por USB: activa «Depuración USB», conecta el cable y acepta el aviso en el celular.").size(12.0).color(MUTED));
                });
            });

            let Some(i) = &info else { return };
            jb_section(ui, &format!("{} {} · Android {}", i.brand, i.model, i.android), "datos leídos del celular ahora mismo", ACCENT);
            ui.columns(3, |c| {
                let used = 1.0 - i.data_free as f32 / i.data_total.max(1) as f32;
                let ram = 1.0 - i.ram_avail as f32 / i.ram_total.max(1) as f32;
                let bat = i.battery as f32 / 100.0;
                for (k, (frac, col, big, label, sub)) in [
                    (used, jb_level_color(used), format!("{:.0}%", used * 100.0), "Almacenamiento", format!("{} libres de {}", fmt::jb_bytes(i.data_free), fmt::jb_bytes(i.data_total))),
                    (ram, jb_level_color(ram), format!("{:.0}%", ram * 100.0), "RAM", format!("{} libres de {}", fmt::jb_bytes(i.ram_avail), fmt::jb_bytes(i.ram_total))),
                    (bat, if bat < 0.2 { DANGER } else if bat < 0.4 { WARN } else { ACCENT }, format!("{}%", i.battery), "Batería", format!("{:.1} °C · salud {}", i.temp_c, i.health)),
                ]
                .into_iter()
                .enumerate()
                {
                    jb_card().jb_show(&mut c[k], |ui| {
                        ui.set_width(ui.available_width());
                        let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 150.0), Sense::hover());
                        jb_ring(ui.painter(), rr.center(), 52.0, frac, col, 7.0);
                        ui.painter().text(rr.center() - vec2(0.0, 6.0), Align2::CENTER_CENTER, big, jb_font(28.0), TEXT);
                        ui.painter().text(rr.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, label, jb_font(10.0), col);
                        ui.vertical_centered(|ui| ui.label(RichText::new(sub).size(12.0).color(MUTED)));
                    });
                }
            });
            ui.columns(2, |c| {
                jb_tinted_card(ACCENT).jb_show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("⚡ Limpieza profunda").font(jb_font(15.0)).color(ACCENT));
                    ui.label(RichText::new("Borra la caché de TODAS las apps de una vez, algo que Android 11 no deja hacer desde el propio celular.").size(12.5));
                    ui.label(RichText::new("No toca fotos, videos, mensajes, cuentas ni datos de las apps.").size(12.0).color(ACCENT));
                    if jb_button(ui, "Borrar caché de todas las apps", ACCENT, vec2(ui.available_width(), 42.0), busy.is_none()).clicked() {
                        act = Some(JbAct::Trim);
                    }
                    if apk.is_some() && jb_button(ui, "Instalar / actualizar NEXO móvil", ACCENT, vec2(ui.available_width(), 42.0), busy.is_none()).clicked() {
                        act = Some(JbAct::Install);
                    }
                });
                jb_card().jb_show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Batería por app · desde la última carga").font(jb_font(13.0)).color(ACCENT));
                    if usage.is_empty() {
                        ui.label(RichText::new("Sin datos todavía (se acumulan con el uso).").color(MUTED));
                    }
                    let max = usage.first().map(|u| u.1).unwrap_or(1.0).max(1.0);
                    for (name, mah) in usage {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).size(12.5));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(format!("{:.0} mAh", mah)).size(12.5).color(WARN));
                            });
                        });
                        jb_bar(ui, (*mah / max) as f32, WARN, 4.0);
                    }
                });
            });
            jb_section(ui, "Apps preinstaladas", "desactivarlas es reversible: no se borran y se pueden volver a activar", WARN);
            jb_card().jb_show(ui, |ui| {
                ui.set_width(ui.available_width());
                if bloat.is_empty() {
                    ui.label(RichText::new("No se encontraron apps preinstaladas innecesarias de la lista conocida.").color(MUTED));
                }
                for b in bloat {
                    ui.horizontal(|ui| {
                        let mut on = b.enabled;
                        if jb_toggle(ui, &mut on).changed() {
                            act = Some(JbAct::Toggle(b.pkg.clone(), on));
                        }
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&b.name).font(jb_font(14.5)).color(if b.enabled { TEXT } else { MUTED }));
                            ui.label(RichText::new(format!("{}  ·  {}", b.why, b.pkg)).size(11.5).color(MUTED));
                        });
                    });
                }
            });
        });

        let serial = self.dev_sel.clone().unwrap_or_default();
        match act {
            Some(JbAct::Refresh) => self.jb_adb_refresh(),
            Some(JbAct::Select(s)) => self.jb_adb_load(s),
            Some(JbAct::Pair) => {
                let (a, c) = (self.pair_addr.clone(), self.pair_code.clone());
                self.pair_code.clear();
                self.jb_adb_spawn("Vinculando…", move || JbAdbMsg::Done(adb::jb_pair(&a, &c), true));
            }
            Some(JbAct::Connect) => {
                let a = self.conn_addr.clone();
                self.jb_adb_spawn("Conectando…", move || JbAdbMsg::Done(adb::jb_connect(&a), true));
            }
            Some(JbAct::Trim) => self.jb_adb_spawn("Borrando la caché de todas las apps…", move || {
                JbAdbMsg::Done(adb::jb_trim_caches(&serial).map(|f| format!("Caché borrada · {} liberados en el celular", fmt::jb_bytes(f))), false)
            }),
            Some(JbAct::Install) => {
                if let Some(apk) = apk {
                    self.jb_adb_spawn("Instalando NEXO móvil…", move || JbAdbMsg::Done(adb::jb_install_apk(&serial, &apk), false));
                }
            }
            Some(JbAct::Toggle(pkg, on)) => self.jb_adb_spawn(if on { "Activando…" } else { "Desactivando…" }, move || {
                JbAdbMsg::Done(adb::jb_set_enabled(&serial, &pkg, on).map(|_| if on { "App activada de nuevo.".to_string() } else { "App desactivada (puedes reactivarla cuando quieras).".to_string() }), false)
            }),
            None => {}
        }
    }

    // memoria RAM (fase 2)

    /// jimy-betanzos: Mide los programas en otro hilo: la ventana no se traba cada 2 segundos.
    fn jb_refresh_ram(&mut self, now: f64) {
        if self.ram_pending {
            return;
        }
        self.ram_pending = true;
        self.last_sample = now;
        let sampler = self.sampler.clone();
        self.jb_bg(move || {
            let (groups, cpu) = sampler.lock().map(|mut s| s.jb_sample(true)).unwrap_or_default();
            let alerts = config::jb_load_alerts();
            let running = vigia::jb_is_running();
            Box::new(move |a: &mut JbNexoApp, now| {
                a.groups = groups;
                a.cpu = cpu;
                a.snap = jb_snapshot(now);
                a.ram_hist.push_back(a.snap.mem.load as f32);
                if a.ram_hist.len() > 90 {
                    a.ram_hist.pop_front();
                }
                a.vigia_running = running;
                a.alerts = alerts;
                a.ram_pending = false;
            })
        });
    }

    /// jimy-betanzos: Activa o desactiva el vigía en segundo plano.
    fn jb_set_vigia(&mut self, on: bool, now: f64) {
        if self.pending_actions > 0 {
            return;
        }
        self.jb_show_toast(if on { "Activando el vigía…" } else { "Deteniendo el vigía…" }.into(), ACCENT, now);
        self.jb_bg_action("Configurando el vigía", move |_progress| {
            let res: Result<(), String> = if on {
                install::jb_install().map(|_| {
                    install::jb_set_autostart(true);
                    if !vigia::jb_is_running() {
                        vigia::jb_start();
                        std::thread::sleep(Duration::from_millis(800));
                    }
                })
            } else {
                install::jb_set_autostart(false);
                vigia::jb_stop_and_wait();
                Ok(())
            };
            let running = vigia::jb_is_running();
            Box::new(move |a: &mut JbNexoApp, now| {
                a.vigia_running = running;
                a.last_sample = -10.0;
                match res {
                    Ok(()) => {
                        a.cfg.vigia_enabled = on;
                        a.cfg.jb_save();
                        if on {
                            a.jb_show_toast("Vigía activado: busca el icono de NEXO junto al reloj".into(), ACCENT, now);
                        } else {
                            a.jb_show_toast("Vigía desactivado".into(), WARN, now);
                        }
                    }
                    Err(e) => a.jb_show_toast(format!("No se pudo activar el vigía: {}", e), DANGER, now),
                }
            })
        });
    }

    /// jimy-betanzos: Pantalla Memoria RAM.
    fn jb_page_ram(&mut self, ui: &mut Ui) {
        jb_header(ui, "› Estabilizador", "Memoria RAM", "Vigila la memoria y actúa solo cuando hay un problema real. Nunca cierra nada sin tu permiso.");
        let now = ui.input(|i| i.time);
        let mem_load = self.snap.mem.load;
        let frac = mem_load as f32 / 100.0;
        let (avail, total) = (self.snap.mem.avail, self.snap.mem.total);
        let commit = self.snap.mem.jb_commit_pct();
        let cpu = self.cpu;
        let apps = self.groups.iter().filter(|g| !g.windows.is_empty()).count();
        let hist: Vec<f32> = self.ram_hist.iter().copied().collect();
        let trimmable = self.groups.iter().filter(|g| g.jb_can_trim()).count();
        let vigia_running = self.vigia_running;
        let mut do_stabilize = false;
        let mut action: Option<(String, bool)> = None;
        let mut vigia_toggle: Option<bool> = None;
        let mut cfg_changed = false;
        let mut test_toast = false;
        let groups = &self.groups;
        let alerts = &self.alerts;
        let cfg = &mut self.cfg;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(3, |c| {
                jb_card().jb_show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 196.0), Sense::hover());
                    let col = jb_level_color(frac);
                    jb_ring(ui.painter(), r.center(), 70.0, frac, col, 9.0);
                    ui.painter().text(r.center() - vec2(0.0, 8.0), Align2::CENTER_CENTER, format!("{}%", mem_load), jb_font(36.0), TEXT);
                    ui.painter().text(r.center() + vec2(0.0, 22.0), Align2::CENTER_CENTER, "RAM en uso", jb_font(11.0), col);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(format!("{} libres de {}", fmt::jb_bytes(avail), fmt::jb_bytes(total))).size(12.5).color(MUTED));
                    });
                });
                jb_card().jb_show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Memoria virtual").font(jb_font(12.0)).color(MUTED));
                    ui.label(RichText::new(format!("{:.0} %", commit)).font(jb_font(20.0)));
                    jb_bar(ui, commit / 100.0, jb_level_color(commit / 100.0), 6.0);
                    ui.label(RichText::new("Procesador").font(jb_font(12.0)).color(MUTED));
                    ui.label(RichText::new(format!("{:.0} %", cpu)).font(jb_font(20.0)));
                    jb_bar(ui, cpu / 100.0, jb_level_color(cpu / 100.0), 6.0);
                    ui.label(RichText::new(format!("RAM · últimos 3 min · {} programas con ventana", apps)).size(11.5).color(MUTED));
                    jb_sparkline(ui, &hist, ACCENT);
                });
                c[2].with_layout(Layout::top_down(Align::Min), |ui| {
                    jb_tinted_card(ACCENT).jb_show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("⚡ Estabilizar").font(jb_font(16.0)).color(ACCENT));
                        ui.label(RichText::new(if cfg!(windows) { "Compacta los programas inactivos de segundo plano: su memoria pasa a la reserva de Windows y vuelve sola si la necesitan." } else { "Libera la caché de memoria del sistema (archivos guardados por si acaso). Te pedirá tu contraseña." }).size(12.5));
                        ui.add_space(2.0);
                        ui.label(RichText::new("No cierra nada · no pierdes datos · no toca la app que usas.").size(12.0).color(ACCENT));
                        ui.add_space(8.0);
                        let w = ui.available_width();
                        let label = if cfg!(windows) { format!("Estabilizar ahora · {}", trimmable) } else { "Estabilizar ahora".to_string() };
                        if jb_button(ui, &label, ACCENT, vec2(w, 46.0), trimmable > 0 || cfg!(unix)).clicked() {
                            do_stabilize = true;
                        }
                    });
                });
            });

            jb_section(ui, "Programas que más memoria usan", "agrupados por programa · se actualiza cada 2 s", ACCENT);
            jb_card().jb_inner_margin(Margin::symmetric(16.0, 10.0)).jb_show(ui, |ui| {
                ui.set_width(ui.available_width());
                if groups.is_empty() {
                    ui.label(RichText::new("Midiendo…").color(MUTED));
                }
                for g in groups.iter().take(14) {
                    ui.horizontal(|ui| {
                        let wname = (ui.available_width() - 300.0).max(240.0);
                        ui.allocate_ui_with_layout(vec2(wname, 28.0), Layout::left_to_right(Align::Center), |ui| {
                            ui.label(RichText::new(&g.name).font(jb_font(15.0)));
                            if g.pids.len() > 1 {
                                ui.label(RichText::new(format!("×{}", g.pids.len())).size(11.5).color(MUTED));
                            }
                            if g.system {
                                jb_badge(ui, "Sistema", MUTED);
                            }
                            if g.foreground {
                                jb_badge(ui, "En uso", ACCENT);
                            }
                            if g.unsaved {
                                jb_badge(ui, "Sin guardar", ACCENT);
                            } else if g.editor {
                                jb_badge(ui, "Trabajo", ACCENT);
                            }
                            if g.installing {
                                jb_badge(ui, "Instalando", WARN);
                            }
                            if g.jb_io_active() {
                                jb_badge(ui, "Activo", WARN);
                            } else if g.jb_can_trim() {
                                jb_badge(ui, "Inactivo", jb_alpha(ACCENT, 210));
                            }
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if !g.system && (!g.windows.is_empty() || g.accessible) && jb_link(ui, "Cerrar", DANGER).on_hover_text("Antes te avisa si hay algo importante").clicked() {
                                action = Some((g.exe.clone(), true));
                            }
                            if g.jb_can_trim() && jb_link(ui, "Compactar", ACCENT).clicked() {
                                action = Some((g.exe.clone(), false));
                            }
                            ui.label(RichText::new(format!("{:.0} % CPU", g.cpu_pct)).size(12.0).color(MUTED));
                            let share = g.private as f32 / total.max(1) as f32;
                            ui.label(RichText::new(fmt::jb_bytes(g.private)).font(jb_font(16.0)).color(jb_level_color(share * 4.0)));
                        });
                    });
                }
            });

            ui.add_space(6.0);
            ui.columns(2, |c| {
                jb_card().jb_show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let mut on = cfg.vigia_enabled;
                        if jb_toggle(ui, &mut on).changed() {
                            vigia_toggle = Some(on);
                        }
                        ui.label(RichText::new("Vigía en segundo plano").font(jb_font(18.0)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if vigia_running {
                                jb_badge(ui, "En marcha", ACCENT);
                            } else {
                                jb_badge(ui, "Detenido", MUTED);
                            }
                        });
                    });
                    ui.label(RichText::new(format!("{} que revisa cada 30 s y te avisa. Ocupa unos pocos MB y arranca con {}.", if cfg!(windows) { "Un icono junto al reloj" } else { "Un proceso en segundo plano" }, diag::jb_txt("os"))).size(12.5).color(MUTED));
                    ui.add_space(6.0);
                    ui.label(RichText::new("Aviso de RAM alta").font(jb_font(12.0)).color(MUTED));
                    cfg_changed |= ui.add(egui::Slider::new(&mut cfg.ram_warn, 70..=95).suffix(" %")).changed();
                    ui.label(RichText::new("RAM crítica").font(jb_font(12.0)).color(MUTED));
                    let lo = cfg.ram_warn + 2;
                    cfg.ram_critical = cfg.ram_critical.max(lo);
                    cfg_changed |= ui.add(egui::Slider::new(&mut cfg.ram_critical, lo..=98).suffix(" %")).changed();
                    cfg_changed |= ui.checkbox(&mut cfg.auto_stabilize, "Estabilizar solo si la RAM llega a crítica (nunca cierra nada)").changed();
                    cfg_changed |= ui.checkbox(&mut cfg.alert_leaks, "Avisar de fugas de memoria").changed();
                    cfg_changed |= ui.checkbox(&mut cfg.alert_cpu, "Avisar si el procesador pasa 3 min al máximo").changed();
                    cfg_changed |= ui.checkbox(&mut cfg.alert_disk, "Avisar si el disco baja del 10 % libre").changed();
                    ui.label(RichText::new("En pantalla completa (juegos, videos) solo llegan los avisos críticos.").size(12.0).color(MUTED));
                    if jb_link(ui, "Probar una notificación", ACCENT).clicked() {
                        test_toast = true;
                    }
                });
                c[1].with_layout(Layout::top_down(Align::Min), |ui| {
                    jb_card().jb_show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("Últimos avisos del vigía").font(jb_font(13.0)).color(ACCENT));
                        if alerts.is_empty() {
                            ui.label(RichText::new("Sin avisos todavía.").color(MUTED));
                        }
                        for a in alerts.iter().rev().take(8) {
                            ui.horizontal(|ui| {
                                let col = match a.kind.as_str() {
                                    "critica" | "virtual" | "fuga" | "defensor" | "inicio" => DANGER,
                                    "estabilizada" => ACCENT,
                                    _ => WARN,
                                };
                                jb_badge(ui, &a.kind, col);
                                ui.label(RichText::new(fmt::jb_ago(a.ts)).size(11.5).color(MUTED));
                            });
                            ui.label(RichText::new(&a.text).size(12.5));
                            ui.add_space(2.0);
                        }
                    });
                });
            });
        });

        if do_stabilize {
            let groups = self.groups.clone();
            self.jb_show_toast("Estabilizando…".into(), ACCENT, now);
            self.jb_bg_action("Estabilizando la memoria", move |progress| {
                let (n, freed) = procs::jb_trim_idle_progress(&groups, &progress);
                let text = if cfg!(windows) {
                    format!("Compactados {} programas inactivos · {} recuperados. No se cerró nada.", n, fmt::jb_bytes(freed))
                } else if n > 0 {
                    format!("Memoria del sistema liberada · {} recuperados. No se cerró nada.", fmt::jb_bytes(freed))
                } else {
                    "No se liberó memoria: se canceló el permiso.".to_string()
                };
                config::jb_push_alert("estabilizada", &text);
                Box::new(move |a: &mut JbNexoApp, now| {
                    a.jb_show_toast(text, ACCENT, now);
                    a.last_sample = -10.0;
                })
            });
        }
        if let Some((exe, close)) = action {
            if let Some(g) = self.groups.iter().find(|g| g.exe == exe).cloned() {
                if close {
                    self.close = Some(JbCloseState { g, sent_at: None, ack: false, force_ack: false, done: None });
                } else {
                    self.jb_bg_action("Compactando el programa", move |_progress| {
                        let f = procs::jb_trim(&g);
                        Box::new(move |a: &mut JbNexoApp, now| {
                            a.jb_show_toast(format!("{} compactado · {} recuperados", g.name, fmt::jb_bytes(f)), ACCENT, now);
                            a.last_sample = -10.0;
                        })
                    });
                }
            }
        }
        if let Some(on) = vigia_toggle {
            self.jb_set_vigia(on, now);
        }
        if cfg_changed {
            self.cfg_dirty = Some(now);
        }
        if test_toast {
            notify::jb_toast("Prueba de aviso", "Así se verán los avisos de NEXO.", "Es solo una prueba: no pasa nada.");
        }
    }

    /// jimy-betanzos: Ventana para cerrar un programa con cuidado.
    fn jb_close_modal(&mut self, ctx: &egui::Context, now: f64) {
        let exe = self.close.as_ref().map(|c| c.g.exe.clone()).unwrap_or_default();
        let alive = sys::jb_running_processes().contains(&exe);
        let Some(st) = self.close.as_mut() else { return };
        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("backdrop_close"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(200));
            });

        let g = st.g.clone();
        let reasons = g.jb_reasons();
        let important = !reasons.is_empty();
        let has_win = !g.windows.is_empty();
        let waited = st.sent_at.map(|t| now - t);
        if st.sent_at.is_some() && !alive && st.done.is_none() {
            st.done = Some(format!("✔ {} se cerró correctamente.", g.name));
        }
        let (mut cancel, mut send, mut force) = (false, false, false);

        let area = egui::Area::new(egui::Id::new("close_box"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                jb_card().jb_fill(PANEL).jb_stroke(Stroke::new(1.5_f32, jb_alpha(ACCENT, 150))).jb_inner_margin(Margin::same(24.0)).jb_show(ui, |ui| {
                    ui.set_width(580.0);
                    ui.label(RichText::new("› Cerrar programa").font(jb_font(12.0)).color(ACCENT));
                    ui.label(RichText::new(format!("Cerrar {}", g.name)).font(jb_font(24.0)));
                    ui.label(RichText::new(format!("{} procesos · {} en memoria", g.pids.len(), fmt::jb_bytes(g.private))).color(MUTED));
                    ui.add_space(6.0);

                    if let Some(msg) = &st.done {
                        jb_tinted_card(ACCENT).jb_show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(msg).color(ACCENT).font(jb_font(15.0)));
                        });
                        ui.add_space(8.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if jb_button(ui, "Listo", ACCENT, vec2(140.0, 44.0), true).clicked() {
                                cancel = true;
                            }
                        });
                        return;
                    }
                    if g.system {
                        ui.label(RichText::new(format!("Es parte de {}: NEXO no lo cierra.", diag::jb_txt("os"))).color(WARN));
                    }
                    if important {
                        jb_tinted_card(ACCENT).jb_show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new("⚠  Antes de cerrar").font(jb_font(14.0)).color(ACCENT));
                            for r in &reasons {
                                ui.label(RichText::new(format!("•  {}", r)).size(13.0));
                            }
                            if st.sent_at.is_none() {
                                ui.checkbox(&mut st.ack, "Entiendo, quiero cerrarlo igualmente");
                            }
                        });
                        ui.add_space(4.0);
                    }
                    match waited {
                        None if has_win => {
                            ui.label(RichText::new("Cerrar normalmente es como pulsar la X: si hay algo sin guardar, el programa te preguntará.").size(13.0).color(MUTED));
                        }
                        None => {
                            ui.label(RichText::new("No tiene ventana abierta: solo puede cerrarse a la fuerza.").color(WARN));
                        }
                        Some(s) if s < 4.0 => {
                            ui.label(RichText::new("Pidiendo que se cierre… si te pregunta si guardar, respóndele.").color(ACCENT));
                        }
                        Some(_) => {
                            ui.label(RichText::new("Sigue abierto: puede estar esperando tu respuesta (guardar / no guardar) o no responde.").color(WARN));
                        }
                    }
                    let show_force = !g.system && (!has_win || waited.is_some_and(|s| s >= 4.0));
                    if show_force {
                        jb_tinted_card(DANGER).jb_show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new("Forzar el cierre pierde todo lo que no esté guardado.").color(DANGER));
                            ui.checkbox(&mut st.force_ack, "Sí, forzar el cierre aunque pierda cambios");
                        });
                    }
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if show_force && jb_button(ui, "Forzar cierre", DANGER, vec2(180.0, 44.0), st.force_ack && (!important || st.ack || st.sent_at.is_some())).clicked() {
                            force = true;
                        }
                        if !g.system && st.sent_at.is_none() && has_win && jb_button(ui, "Cerrar normalmente", ACCENT, vec2(240.0, 44.0), !important || st.ack).clicked() {
                            send = true;
                        }
                        if jb_button(ui, "Cancelar", MUTED, vec2(140.0, 44.0), true).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        ctx.move_to_top(area.response.layer_id);
        if send {
            procs::jb_close_gracefully(&g);
            st.sent_at = Some(now);
        }
        if force {
            let n = procs::jb_force_kill(&g);
            st.done = Some(if n > 0 {
                format!("{} se cerró a la fuerza.", g.name)
            } else {
                "No se pudo cerrar: puede requerir permisos de administrador.".into()
            });
        }
        if cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.close = None;
            self.last_sample = -10.0;
        }
    }

    /// jimy-betanzos: Dibuja el aviso breve.
    fn jb_toast_ui(&mut self, ctx: &egui::Context, now: f64) {
        let Some((text, col, until)) = self.toast.clone() else { return };
        if now > until {
            self.toast = None;
            return;
        }
        egui::Area::new(egui::Id::new("toast"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::RIGHT_BOTTOM, vec2(-24.0, -24.0))
            .interactable(false)
            .show(ctx, |ui| {
                jb_tinted_card(col).jb_fill(jb_alpha(PANEL, 245)).jb_show(ui, |ui| {
                    ui.label(RichText::new(text).font(jb_font(14.0)).color(col));
                });
            });
        ctx.request_repaint_after(Duration::from_secs_f64((until - now).max(0.05)));
    }
}

/// jimy-betanzos: Color según la nota (normal, aviso o grave).
fn jb_score_color(s: u8) -> Color32 {
    match s {
        85..=100 => ACCENT,
        70..=84 => ACCENT,
        55..=69 => WARN,
        _ => DANGER,
    }
}

/// jimy-betanzos: Fila de comprobación: 0 bien · 1 revisar · 2 importante · 3 informativo.
/// Devuelve si se pulsó su botón de acción.
fn jb_check_row(ui: &mut Ui, state: u8, title: &str, detail: &str, action: Option<&JbAction>) -> bool {
    let (icon, col) = match state {
        0 => ("✔", ACCENT),
        1 => ("⚠", WARN),
        2 => ("⚠", DANGER),
        _ => ("•", ACCENT),
    };
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon).color(col).size(16.0));
        ui.label(RichText::new(title).font(jb_font(14.5)));
        ui.label(RichText::new(detail).size(12.5).color(if state == 0 { MUTED } else { col }));
        if let Some(a) = action {
            if state != 0 {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    clicked = jb_link(ui, a.jb_label(), ACCENT).clicked();
                });
            }
        }
    });
    clicked
}

/// jimy-betanzos: Botón de la barra de título (minimizar, maximizar, cerrar).
fn jb_title_btn(ui: &mut Ui, text: &str, hover: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(42.0, 30.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 6.0, jb_alpha(hover, 50));
    }
    p.text(rect.center(), Align2::CENTER_CENTER, text, egui::FontId::proportional(15.0), if resp.hovered() { hover } else { MUTED });
    resp
}

/// jimy-betanzos: Tarjeta de una categoría con su casilla y tamaño.
fn jb_category_card(ui: &mut Ui, r: &mut JbCatResult, admin: bool, thumbs: &mut JbThumbCache) {
    let color = match r.kind {
        JbKind::Safe => ACCENT,
        JbKind::Review => WARN,
        JbKind::Protected => ACCENT,
    };
    let locked = r.admin_only && !admin;
    let has = r.size > 0 || (r.special == JbSpecial::RecycleBin && r.count > 0);
    let frame = if r.jb_selected_size() > 0 { jb_card().jb_stroke(Stroke::new(1.0_f32, jb_alpha(color, 120))) } else { jb_card() };
    frame.jb_inner_margin(Margin::symmetric(16.0, 12.0)).jb_show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            if r.jb_whole() {
                ui.add_enabled(!locked && has, egui::Checkbox::without_text(&mut r.selected));
            } else {
                let mut all = !r.items.is_empty() && r.items.iter().all(|i| i.selected);
                if ui.add_enabled(has, egui::Checkbox::without_text(&mut all)).changed() {
                    for i in r.items.iter_mut() {
                        i.selected = all;
                    }
                }
            }
            let w = (ui.available_width() - 170.0).max(200.0);
            ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::LEFT), |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(r.name).font(jb_font(16.0)));
                    match r.kind {
                        JbKind::Safe => jb_badge(ui, "Seguro", ACCENT),
                        JbKind::Review => jb_badge(ui, "Revisar", WARN),
                        JbKind::Protected => jb_badge(ui, "Protegido", ACCENT),
                    }
                    if locked {
                        jb_badge(ui, "Admin", ACCENT);
                    }
                });
                ui.label(RichText::new(r.desc).size(12.5).color(MUTED));
                for n in &r.notes {
                    ui.label(RichText::new(format!("⚠ {}", n)).size(12.0).color(WARN));
                }
            });
            ui.with_layout(Layout::top_down(Align::Max), |ui| {
                if has {
                    ui.label(RichText::new(fmt::jb_bytes(r.size)).font(jb_font(20.0)).color(color));
                    ui.label(RichText::new(format!("{} elementos", fmt::jb_count(r.count))).size(12.0).color(MUTED));
                } else if !locked {
                    ui.label(RichText::new("✔ Limpio").font(jb_font(14.0)).color(jb_alpha(ACCENT, 190)));
                }
            });
        });

        if !r.items.is_empty() {
            let whole = r.jb_whole();
            ui.horizontal(|ui| {
                let lbl = if r.expanded {
                    "‹  Ocultar archivos"
                } else if whole {
                    "Ver muestra de archivos  ›"
                } else {
                    "Ver y elegir archivos  ›"
                };
                if jb_link(ui, lbl, ACCENT).clicked() {
                    r.expanded = !r.expanded;
                }
                if r.expanded && !whole {
                    ui.add_space(12.0);
                    if jb_link(ui, "Seleccionar todo", MUTED).clicked() {
                        r.items.iter_mut().for_each(|i| i.selected = true);
                    }
                    if jb_link(ui, "Ninguno", MUTED).clicked() {
                        r.items.iter_mut().for_each(|i| i.selected = false);
                    }
                }
            });
            if r.expanded {
                jb_items_list(ui, r, thumbs);
            }
        }
    });
    ui.add_space(4.0);
}

/// jimy-betanzos: Cuadro de miniatura: la imagen real si existe, o el tipo de archivo mientras carga.
fn jb_draw_thumb(ui: &Ui, rect: egui::Rect, tex: Option<Option<egui::TextureHandle>>, path: &Path) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, jb_alpha(PANEL_HI, 230));
    match tex {
        Some(Some(t)) => {
            let s = t.size_vec2();
            let k = (rect.width() / s.x).min(rect.height() / s.y);
            let r = egui::Rect::from_center_size(rect.center(), s * k);
            p.image(t.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        other => {
            let ext = rules::jb_ext_of(path).to_uppercase();
            let col = if other.is_none() { jb_alpha(ACCENT, 120) } else { MUTED };
            p.text(rect.center(), Align2::CENTER_CENTER, if ext.is_empty() { "?".into() } else { ext }, jb_font(10.5), col);
        }
    }
    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, jb_alpha(ACCENT, 70)));
}

/// jimy-betanzos: Lista de archivos de muestra de una categoría.
fn jb_items_list(ui: &mut Ui, r: &mut JbCatResult, thumbs: &mut JbThumbCache) {
    let whole = r.jb_whole();
    let protected = r.kind == JbKind::Protected;
    let total = r.count;
    let id = r.id;
    let n = r.items.len();
    ui.label(RichText::new("Señala un archivo para verlo en el visor de la derecha · «Ver» lo abre con su programa.").size(11.5).color(MUTED));
    let mut hovered: Option<usize> = None;
    let height = 330.0;
    ui.horizontal_top(|ui| {
        let list_w = (ui.available_width() * 0.6).max(360.0);
        // Lista con miniaturas (solo se dibujan y cargan las filas visibles)
        ui.allocate_ui_with_layout(vec2(list_w, height), Layout::top_down(Align::LEFT), |ui| {
            let items = &mut r.items;
            ScrollArea::vertical()
                .id_salt(id)
                .max_height(height)
                .auto_shrink([false, false])
                .show_rows(ui, 50.0, n, |ui, range| {
                    for i in range {
                        let it = &mut items[i];
                        let row = ui.horizontal(|ui| {
                            ui.set_min_height(46.0);
                            if !whole {
                                ui.checkbox(&mut it.selected, "");
                            }
                            let (tr, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
                            jb_draw_thumb(ui, tr, thumbs.jb_get(&it.path, SMALL), &it.path);
                            let w = (ui.available_width() - 120.0).max(120.0);
                            ui.allocate_ui_with_layout(vec2(w, 46.0), Layout::top_down(Align::LEFT), |ui| {
                                let name = it.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                                let (icon, col) = match rules::jb_protection(&it.path) {
                                    JbProtect::Video => ("🎬 ", ACCENT),
                                    JbProtect::Document => ("📄 ", ACCENT),
                                    JbProtect::No => ("", TEXT),
                                };
                                ui.add(egui::Label::new(RichText::new(format!("{}{}", icon, name)).size(13.5).color(col)).truncate());
                                let folder = it.path.parent().map(|d| fmt::jb_short_path(d, 60)).unwrap_or_default();
                                ui.add(egui::Label::new(RichText::new(folder).monospace().size(10.5).color(MUTED)).truncate());
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if jb_link(ui, "Ver", ACCENT).on_hover_text("Abrir con su programa").clicked() {
                                    sys::jb_open_file(&it.path);
                                }
                                ui.label(RichText::new(fmt::jb_bytes(it.size)).size(12.0));
                            });
                        });
                        if ui.rect_contains_pointer(row.response.rect) {
                            hovered = Some(i);
                            ui.painter().rect_stroke(row.response.rect.expand(1.0), 0.0, Stroke::new(1.0_f32, jb_alpha(ACCENT, 160)));
                        }
                    }
                });
        });
        if hovered.is_some() {
            r.preview = hovered;
        }
        // Visor
        let shown = r.preview.filter(|i| *i < n).unwrap_or(0);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::top_down(Align::Center), |ui| {
            jb_tinted_card(ACCENT).jb_inner_margin(Margin::same(12.0)).jb_show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(height - 26.0);
                ui.label(RichText::new("Visor").font(jb_font(11.0)).color(ACCENT));
                let Some(it) = r.items.get_mut(shown) else { return };
                let box_w = ui.available_width();
                let box_h = 190.0;
                let (bx, _) = ui.allocate_exact_size(vec2(box_w, box_h), Sense::hover());
                let p = ui.painter();
                p.rect_filled(bx, 0.0, jb_alpha(BG, 200));
                match thumbs.jb_get(&it.path, BIG) {
                    Some(Some(t)) => {
                        let s = t.size_vec2();
                        let k = (bx.width() / s.x).min(bx.height() / s.y).min(1.5);
                        let rr = egui::Rect::from_center_size(bx.center(), s * k);
                        p.image(t.id(), rr, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    Some(None) => {
                        p.text(bx.center(), Align2::CENTER_CENTER, "Sin vista previa", jb_font(13.0), MUTED);
                    }
                    None => {
                        p.text(bx.center(), Align2::CENTER_CENTER, "Cargando…", jb_font(13.0), ACCENT);
                    }
                }
                p.rect_stroke(bx, 0.0, Stroke::new(1.0_f32, jb_alpha(ACCENT, 80)));
                let name = it.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                ui.add(egui::Label::new(RichText::new(name).font(jb_font(14.0))).truncate());
                let when = std::fs::metadata(&it.path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| fmt::jb_date(d.as_secs()))
                    .unwrap_or_default();
                ui.label(RichText::new(format!("{} · {}", fmt::jb_bytes(it.size), when)).size(11.5).color(MUTED));
                if protected {
                    ui.label(RichText::new(format!("Origen: {}", it.origin)).size(11.5).color(ACCENT));
                }
                ui.horizontal(|ui| {
                    if jb_link(ui, "Abrir", ACCENT).clicked() {
                        sys::jb_open_file(&it.path);
                    }
                    if jb_link(ui, "Mostrar en carpeta", ACCENT).clicked() {
                        sys::jb_reveal_in_explorer(&it.path);
                    }
                    if !whole {
                        let (txt, col) = if it.selected { ("Marcado: se borrará", DANGER) } else { ("Se conserva", ACCENT) };
                        if jb_link(ui, txt, col).on_hover_text("Cambiar").clicked() {
                            it.selected = !it.selected;
                        }
                    }
                });
            });
        });
    });
    if whole && total > n as u64 {
        ui.label(RichText::new(format!("… y {} archivos más (se muestra una muestra)", fmt::jb_count(total - n as u64))).size(12.0).color(MUTED));
    }
}

/// jimy-betanzos: Permite cambiar el tamaño de la ventana desde los bordes.
fn jb_resize_edges(ctx: &egui::Context) {
    // jimy-betanzos - dependencia: interfaz gráfica (egui)
    use egui::{CursorIcon, ResizeDirection as D};
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let rect = ctx.screen_rect();
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else { return };
    let m = 6.0;
    let (l, r, t, b) = (pos.x < rect.left() + m, pos.x > rect.right() - m, pos.y < rect.top() + m, pos.y > rect.bottom() - m);
    let dir = match (l, r, t, b) {
        (true, _, true, _) => Some((D::NorthWest, CursorIcon::ResizeNwSe)),
        (_, true, true, _) => Some((D::NorthEast, CursorIcon::ResizeNeSw)),
        (true, _, _, true) => Some((D::SouthWest, CursorIcon::ResizeNeSw)),
        (_, true, _, true) => Some((D::SouthEast, CursorIcon::ResizeNwSe)),
        (true, ..) => Some((D::West, CursorIcon::ResizeHorizontal)),
        (_, true, ..) => Some((D::East, CursorIcon::ResizeHorizontal)),
        (_, _, true, _) => Some((D::North, CursorIcon::ResizeVertical)),
        (_, _, _, true) => Some((D::South, CursorIcon::ResizeVertical)),
        _ => None,
    };
    if let Some((d, icon)) = dir {
        ctx.set_cursor_icon(icon);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(d));
        }
    }
}

/// jimy-betanzos: Implementación de eframe::App para NexoApp.
impl eframe::App for JbNexoApp {
    /// jimy-betanzos: Dibuja la ventana en cada cuadro.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.jb_update_inner(ctx);
    }
}

/// jimy-betanzos: Métodos de NexoApp.
impl JbNexoApp {
    /// jimy-betanzos: Modo de documentación (no lo usa el usuario): con `NEXO_CAPTURA=archivo.bmp`, NEXO
    /// se fotografía a sí mismo pasados `NEXO_CAPTURA_SEG` segundos y se cierra. Con
    /// `NEXO_CAPTURA_ESCANEAR=1` antes lanza un escaneo (solo lectura) para mostrar resultados.
    fn jb_capture_step(&mut self, ctx: &egui::Context, now: f64) {
        // jimy-betanzos - dependencia: biblioteca estándar de Rust (sync)
        use std::sync::atomic::{AtomicU8, Ordering};
        static STATE: AtomicU8 = AtomicU8::new(0); // 0 inicio · 1 esperando · 2 pedida · 3 lista
        let Some(out) = std::env::var_os("NEXO_CAPTURA") else { return };
        let delay: f64 = std::env::var("NEXO_CAPTURA_SEG").ok().and_then(|s| s.parse().ok()).unwrap_or(4.0);
        match STATE.load(Ordering::Relaxed) {
            0 => {
                if std::env::var_os("NEXO_CAPTURA_ESCANEAR").is_some() {
                    self.jb_spawn_scan(None, false);
                }
                STATE.store(1, Ordering::Relaxed);
            }
            1 if now >= delay && self.busy.is_none() => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                STATE.store(2, Ordering::Relaxed);
            }
            2 => {
                let shot = ctx.input(|i| {
                    i.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                if let Some(img) = shot {
                    let _ = std::fs::write(&out, crate::thumbs::jb_bmp(img.size[0], img.size[1], img.as_raw()));
                    STATE.store(3, Ordering::Relaxed);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            _ => {}
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    /// jimy-betanzos: Contenido principal de cada cuadro.
    fn jb_update_inner(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let t = now as f32;
        self.jb_poll(now);
        if now - self.snap.at > 2.5 {
            self.snap = jb_snapshot(now);
        }

        self.now = now;
        self.jb_poll_jobs(now);
        if self.thumbs.jb_poll(ctx) {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
        self.jb_poll_phase3(now);
        self.jb_poll_adb(now);
        self.jb_capture_step(ctx, now);

        // La lista de programas solo se mide mientras la pantalla de RAM está abierta.
        if self.page == JbPage::Ram && now - self.last_sample > 2.0 {
            self.jb_refresh_ram(now);
        }

        let screen = ctx.screen_rect();
        jb_background(&ctx.layer_painter(egui::LayerId::background()), screen);
        self.jb_titlebar(ctx);
        self.jb_sidebar(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().inner_margin(Margin { left: 28.0, right: 28.0, top: 14.0, bottom: 18.0 }))
            .show(ctx, |ui| match self.page {
                JbPage::Panel => self.jb_page_panel(ui),
                JbPage::Clean => self.jb_page_clean(ui, t),
                JbPage::Auto => self.jb_page_auto(ui, t),
                JbPage::History => self.jb_page_history(ui),
                JbPage::Ram => self.jb_page_ram(ui),
                JbPage::Diag => self.jb_page_diag(ui, t),
                JbPage::Shield => self.jb_page_shield(ui, t),
                JbPage::Mobile => self.jb_page_mobile(ui, t),
            });

        if self.confirm {
            self.jb_confirm_modal(ctx);
        }
        if self.close.is_some() {
            self.jb_close_modal(ctx, now);
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        self.jb_ops_view(ctx, now);
        self.jb_toast_ui(ctx, now);
        let frame_p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("border")));
        frame_p.rect_stroke(screen.shrink(0.5), 0.0, Stroke::new(1.0_f32, PANEL_HI));
        jb_resize_edges(ctx);

        // Tras arrancar, terminar un trabajo o minimizar: soltar la memoria que ya no se usa.
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        if minimized && !self.was_minimized {
            self.trim_in = 1;
        }
        self.was_minimized = minimized;
        if self.busy.is_none() && self.trim_in > 0 {
            self.trim_in -= 1;
            if self.trim_in == 0 {
                sys::jb_trim_own_memory();
            } else {
                ctx.request_repaint_after(Duration::from_millis(400));
            }
        }

        // Repintar solo cuando hace falta: así la CPU queda en ~0 % en reposo.
        if let Some(t) = self.cfg_dirty {
            if now - t > 0.5 && !ctx.input(|i| i.pointer.any_down()) {
                self.cfg.jb_save();
                self.cfg_dirty = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(300));
            }
        }
        if !self.jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.busy.is_some() || self.diag_rx.is_some() || (self.adb_rx.is_some() && self.page == JbPage::Mobile) || (self.mpscan_rx.is_some() && self.page == JbPage::Shield) {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if self.mpscan_rx.is_some() {
            ctx.request_repaint_after(Duration::from_secs(1));
        } else if self.page == JbPage::Ram {
            ctx.request_repaint_after(Duration::from_secs(2));
        } else if self.page == JbPage::Panel {
            ctx.request_repaint_after(Duration::from_secs(3));
        }
    }
}
