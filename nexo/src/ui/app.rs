// ════════════════════════════════════════════════════════════════════════
//  NEXO · nexo/src/ui/app.rs
//  Autor: Chatta Betanzos, Jimy Alexander  ·  jimy-betanzos
// ════════════════════════════════════════════════════════════════════════

// jimy-betanzos · dependencia: biblioteca estándar de Rust (collections)
use std::collections::VecDeque;
// jimy-betanzos · dependencia: biblioteca estándar de Rust (path)
use std::path::Path;
// jimy-betanzos · dependencia: biblioteca estándar de Rust (sync)
use std::sync::atomic::{AtomicBool, Ordering};
// jimy-betanzos · dependencia: biblioteca estándar de Rust (sync)
use std::sync::mpsc::{channel, Receiver};
// jimy-betanzos · dependencia: biblioteca estándar de Rust (sync)
use std::sync::{Arc, Mutex};
// jimy-betanzos · dependencia: biblioteca estándar de Rust (time)
use std::time::Duration;

// jimy-betanzos · dependencia: ventana e interfaz gráfica (eframe/egui)
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, Layout, Margin, RichText, ScrollArea, Sense, Stroke, Ui,
    ViewportCommand,
};

// jimy-betanzos · dependencia: módulo propio de la interfaz (theme)
use super::theme::*;
// jimy-betanzos · dependencia: módulo propio de la interfaz (thumbcache)
use super::thumbcache::{ThumbCache, BIG, SMALL};
// jimy-betanzos · dependencia: módulo propio de la interfaz (widgets)
use super::widgets::*;
// jimy-betanzos · dependencia: módulo propio clean
use crate::clean::{self, CleanReport};
// jimy-betanzos · dependencia: módulo propio config
use crate::config::{self, Config, Frequency, HistoryEntry};
// jimy-betanzos · dependencia: módulo propio rules
use crate::rules::{self, CategoryDef, Env, Kind, Protect, Special};
// jimy-betanzos · dependencia: módulo propio procs
use crate::procs::{self, AppGroup, Sampler};
// jimy-betanzos · dependencia: módulo propio scan
use crate::scan::{self, CatResult, Ctx, Progress, Shared};
// jimy-betanzos · dependencia: módulo propio diag
use crate::diag::{self, Action, Report};
// jimy-betanzos · dependencia: módulo propio
use crate::{adb, fmt, install, notify, schedule, sys, vigia};

/// jimy-betanzos · Resultado de un trabajo en segundo plano: se aplica en el hilo de la interfaz.
type Apply = Box<dyn FnOnce(&mut NexoApp, f64) + Send>;

/// Avance desconocido (barra animada en lugar de porcentaje).
const INDET: u32 = u32::MAX;

/// jimy-betanzos · Operación en curso que se muestra en el panel "En curso".
struct Op {
    id: u64,
    label: String,
    frac: Arc<std::sync::atomic::AtomicU32>,
    started: f64,
}

/// jimy-betanzos · Cierre de un programa en curso (ventana de confirmación).
struct CloseState {
    g: AppGroup,
    sent_at: Option<f64>,
    ack: bool,
    force_ack: bool,
    done: Option<String>,
}

/// jimy-betanzos · Pantallas de la aplicación (Panel, Limpieza, Automático…).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Panel,
    Clean,
    Auto,
    History,
    Ram,
    Diag,
    Shield,
    Mobile,
}

/// jimy-betanzos · Resultado de simular el modo automático: (bytes, archivos, detalle por categoría).
type Simulation = (u64, u64, Vec<(String, u64)>);

/// jimy-betanzos · Trabajo en curso: escaneo, limpieza o simulación.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Busy {
    Scan,
    Clean,
    Simulate,
}

/// jimy-betanzos · Mensajes que envían los hilos de trabajo a la ventana.
enum Msg {
    Scanned(Vec<CatResult>),
    Cleaned(CleanReport),
    Simulated(Vec<CatResult>),
}

/// jimy-betanzos · Datos del equipo que se muestran en el Panel.
struct Snapshot {
    mem: sys::Mem,
    disk_free: u64,
    disk_total: u64,
    power: sys::Power,
    at: f64,
}

/// jimy-betanzos · Toma los datos actuales de disco, memoria y batería.
fn snapshot(now: f64) -> Snapshot {
    let (free, total) = sys::disk_space(&sys::system_drive());
    Snapshot { mem: sys::memory(), disk_free: free, disk_total: total, power: sys::power(), at: now }
}

/// jimy-betanzos · Estado completo de la ventana de NEXO.
pub struct NexoApp {
    page: Page,
    env: Arc<Env>,
    defs: Arc<Vec<CategoryDef>>,
    admin: bool,
    model: String,
    results: Option<Vec<CatResult>>,
    progress: Shared,
    cancel: Arc<AtomicBool>,
    rx: Option<Receiver<Msg>>,
    busy: Option<Busy>,
    report: Option<CleanReport>,
    confirm: bool,
    ack_protected: bool,
    cfg: Config,
    task_registered: bool,
    history: Vec<HistoryEntry>,
    sim: Option<Simulation>,
    toast: Option<(String, Color32, f64)>,
    snap: Snapshot,
    trim_in: u32,
    was_minimized: bool,
    // Fase 2: memoria
    sampler: Arc<Mutex<Sampler>>,
    ram_pending: bool,
    jobs: Vec<Receiver<Apply>>,
    pending_actions: u32,
    cfg_dirty: Option<f64>,
    groups: Vec<AppGroup>,
    cpu: f32,
    ram_hist: VecDeque<f32>,
    last_sample: f64,
    close: Option<CloseState>,
    vigia_running: bool,
    alerts: Vec<config::AlertEntry>,
    // Fase 3: diagnóstico y protección
    diag: Option<Report>,
    diag_rx: Option<Receiver<Result<Report, String>>>,
    mpscan_rx: Option<Receiver<(Result<i32, String>, u64)>>,
    mpscan_msg: Option<(String, Color32)>,
    // Fase 4: celulares (ADB)
    adb_rx: Option<Receiver<AdbMsg>>,
    adb_busy: Option<String>,
    adb_msg: Option<(String, Color32)>,
    adb_scanned: bool,
    devices: Vec<adb::Dev>,
    dev_sel: Option<String>,
    dev_info: Option<adb::Info>,
    dev_bloat: Vec<adb::Bloat>,
    dev_usage: Vec<(String, f64)>,
    pair_addr: String,
    pair_code: String,
    conn_addr: String,
    uninstall_armed: bool,
    thumbs: ThumbCache,
    ops: Vec<Op>,
    next_op: u64,
    now: f64,
    busy_started: f64,
    diag_started: f64,
    diag_prog: Arc<std::sync::atomic::AtomicU32>,
    mp_started: f64,
    adb_started: f64,
}

/// jimy-betanzos · Mensajes de las tareas con celulares.
enum AdbMsg {
    Devices(Result<Vec<adb::Dev>, String>),
    Loaded(String, Result<adb::Info, String>, Vec<adb::Bloat>, Vec<(String, f64)>),
    Done(Result<String, String>, bool),
}

/// jimy-betanzos · Métodos de NexoApp.
impl NexoApp {
    /// jimy-betanzos · Prepara la ventana, el estilo y los datos iniciales.
    pub fn new(cc: &eframe::CreationContext, start_page: Option<String>) -> Self {
        setup(&cc.egui_ctx);
        let env = Env::detect();
        let defs = rules::categories(&env);
        let page = match start_page.as_deref() {
            Some("ram") => Page::Ram,
            Some("limpieza") => Page::Clean,
            Some("diagnostico") => Page::Diag,
            Some("proteccion") => Page::Shield,
            Some("celulares") => Page::Mobile,
            Some("automatico") => Page::Auto,
            Some("historial") => Page::History,
            _ => Page::Panel,
        };
        let mut app = NexoApp {
            page,
            env: Arc::new(env),
            defs: Arc::new(defs),
            admin: sys::is_admin(),
            model: sys::machine_model(),
            results: None,
            progress: Arc::new(Mutex::new(Progress::default())),
            cancel: Arc::new(AtomicBool::new(false)),
            rx: None,
            busy: None,
            report: None,
            confirm: false,
            ack_protected: false,
            cfg: Config::load(),
            task_registered: false,
            history: config::load_history(),
            sim: None,
            toast: None,
            snap: snapshot(0.0),
            trim_in: 4,
            was_minimized: false,
            sampler: Arc::new(Mutex::new(Sampler::default())),
            ram_pending: false,
            jobs: Vec::new(),
            pending_actions: 0,
            cfg_dirty: None,
            groups: Vec::new(),
            cpu: 0.0,
            ram_hist: VecDeque::new(),
            last_sample: -10.0,
            close: None,
            vigia_running: vigia::is_running(),
            alerts: Vec::new(),
            diag: diag::load(),
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
            thumbs: ThumbCache::new(),
            ops: Vec::new(),
            next_op: 1,
            now: 0.0,
            busy_started: 0.0,
            diag_started: 0.0,
            diag_prog: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            mp_started: 0.0,
            adb_started: 0.0,
        };
        app.check_task();
        // Si ya estaba instalado con una versión anterior, se actualiza sin molestar.
        app.bg(|| {
            if install::needs_update() {
                let _ = install::install();
            }
            Box::new(|_: &mut NexoApp, _| {})
        });
        app
    }

    /// jimy-betanzos · Comprueba si la tarea programada sigue registrada.
    fn check_task(&mut self) {
        self.bg(|| {
            let reg = schedule::is_registered();
            Box::new(move |a: &mut NexoApp, _| a.task_registered = reg)
        });
    }

    /// jimy-betanzos · Ejecuta `work` en otro hilo; lo que devuelve se aplica luego en la interfaz.
    fn bg(&mut self, work: impl FnOnce() -> Apply + Send + 'static) {
        let (tx, rx) = channel();
        self.jobs.push(rx);
        std::thread::spawn(move || {
            sys::interactive_thread();
            let _ = tx.send(work());
        });
    }

    /// jimy-betanzos · Igual que `bg`, para acciones del usuario: aparecen en el panel "En curso" y, mientras
    /// corren, sus botones se desactivan. `work` recibe dónde informar su avance (milésimas).
    fn bg_action(&mut self, label: &str, work: impl FnOnce(Arc<std::sync::atomic::AtomicU32>) -> Apply + Send + 'static) {
        self.pending_actions += 1;
        let id = self.next_op;
        self.next_op += 1;
        let frac = Arc::new(std::sync::atomic::AtomicU32::new(INDET));
        self.ops.push(Op { id, label: label.to_string(), frac: frac.clone(), started: self.now });
        self.bg(move || {
            let f = work(frac);
            Box::new(move |a: &mut NexoApp, now| {
                a.pending_actions = a.pending_actions.saturating_sub(1);
                a.ops.retain(|o| o.id != id);
                f(a, now)
            })
        });
    }

    /// jimy-betanzos · Panel "En curso": todo lo que NEXO está haciendo, con su avance y tiempo.
    fn ops_view(&mut self, ctx: &egui::Context, now: f64) {
        // jimy-betanzos · dependencia: biblioteca estándar de Rust (sync)
        use std::sync::atomic::Ordering;
        let mut rows: Vec<(String, Option<f32>, f64, Color32)> = Vec::new();
        if let Some(b) = self.busy {
            let p = self.progress.lock().map(|p| p.clone()).unwrap_or_default();
            let frac = if b == Busy::Clean && p.total_bytes > 0 {
                (p.bytes as f32 / p.total_bytes as f32).min(0.99)
            } else if p.steps > 0 {
                (p.step as f32 / p.steps as f32).min(0.99)
            } else {
                0.0
            };
            let label = match b {
                Busy::Scan => format!("Escaneando · {} archivos", fmt::count(p.files)),
                Busy::Clean => format!("Limpiando · {} liberados", fmt::bytes(p.bytes)),
                Busy::Simulate => "Simulando la limpieza automática".to_string(),
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
            rows.push((format!("Análisis de {} (tiempo estimado)", diag::txt("av")), Some(f), self.mp_started, ACCENT));
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
                tinted_card(ACCENT).fill(alpha(PANEL, 245)).inner_margin(Margin::same(12.0)).show(ui, |ui| {
                    ui.set_width(330.0);
                    ui.label(RichText::new("En curso").font(font(11.0)).color(ACCENT));
                    for (label, frac, started, col) in rows {
                        let elapsed = (now - started).max(0.0);
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(RichText::new(label).size(12.5)).truncate());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let txt = frac.map(|f| format!("{:.0} %", f * 100.0)).unwrap_or_else(|| "…".into());
                                ui.label(RichText::new(txt).font(font(13.0)).color(col));
                            });
                        });
                        progress_bar(ui, frac, col, t);
                        let eta = match frac {
                            Some(f) if f > 0.04 && elapsed > 1.0 => format!(" · quedan ≈ {}", fmt::secs(elapsed * (1.0 - f as f64) / f as f64)),
                            _ => String::new(),
                        };
                        ui.label(RichText::new(format!("{} transcurridos{}", fmt::secs(elapsed), eta)).size(11.0).color(MUTED));
                    }
                });
            });
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    /// jimy-betanzos · Revisa el avance de los trabajos en segundo plano.
    fn poll_jobs(&mut self, now: f64) {
        let mut done: Vec<Apply> = Vec::new();
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

    /// jimy-betanzos · Muestra un aviso breve en la parte inferior.
    fn show_toast(&mut self, text: String, color: Color32, now: f64) {
        self.toast = Some((text, color, now + 5.0));
    }

    /// jimy-betanzos · Recibe los resultados del escaneo y la limpieza.
    fn poll(&mut self, now: f64) {
        let msg = self.rx.as_ref().and_then(|rx| rx.try_recv().ok());
        let Some(msg) = msg else { return };
        self.rx = None;
        self.busy = None;
        self.trim_in = 3;
        match msg {
            Msg::Scanned(r) => self.results = Some(r),
            Msg::Simulated(r) => {
                let safe = r.iter().filter(|c| c.kind == Kind::Safe);
                let freed = safe.clone().map(|c| c.size).sum();
                let files = safe.clone().map(|c| c.count).sum();
                let per = safe.filter(|c| c.size > 0).map(|c| (c.name.to_string(), c.size)).collect();
                self.sim = Some((freed, files, per));
            }
            Msg::Cleaned(rep) => {
                self.history = config::push_history(HistoryEntry {
                    ts: fmt::now(),
                    mode: "manual".into(),
                    freed: rep.freed,
                    files: rep.files,
                    skipped: rep.skipped,
                    recycled: rep.recycled,
                    details: rep.per_cat.clone(),
                    ..Default::default()
                });
                self.show_toast(format!("Limpieza completada · {} liberados", fmt::bytes(rep.freed)), ACCENT, now);
                self.report = Some(rep);
                self.results = None;
                self.snap = snapshot(now);
            }
        }
    }

    /// jimy-betanzos · Lanza el escaneo en un hilo aparte.
    fn spawn_scan(&mut self, only: Option<Vec<String>>, simulate: bool) {
        if self.busy.is_some() {
            return;
        }
        let (defs, prog, cancel, admin) = (self.defs.clone(), self.progress.clone(), self.cancel.clone(), self.admin);
        cancel.store(false, Ordering::Relaxed);
        if let Ok(mut p) = prog.lock() {
            *p = Progress { label: "Preparando escaneo".into(), ..Default::default() };
        }
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.busy = Some(if simulate { Busy::Simulate } else { Busy::Scan });
        self.busy_started = self.now;
        if !simulate {
            self.report = None;
            self.results = None;
        }
        std::thread::spawn(move || {
            sys::interactive_thread();
            let procs = sys::running_processes();
            let ctx = Ctx { admin, procs: &procs, progress: &prog, cancel: &cancel };
            let r = scan::scan_all(&defs, only.as_deref(), &ctx);
            let _ = tx.send(if simulate { Msg::Simulated(r) } else { Msg::Scanned(r) });
        });
    }

    /// jimy-betanzos · Lanza la limpieza en un hilo aparte.
    fn spawn_clean(&mut self) {
        let Some(results) = self.results.clone() else { return };
        if self.busy.is_some() {
            return;
        }
        let (defs, env, prog, cancel, admin) =
            (self.defs.clone(), self.env.clone(), self.progress.clone(), self.cancel.clone(), self.admin);
        cancel.store(false, Ordering::Relaxed);
        let total: u64 = results.iter().map(|r| r.selected_size()).sum();
        if let Ok(mut p) = prog.lock() {
            *p = Progress { label: "Iniciando limpieza".into(), total_bytes: total, ..Default::default() };
        }
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.busy = Some(Busy::Clean);
        self.busy_started = self.now;
        std::thread::spawn(move || {
            sys::interactive_thread();
            let procs = sys::running_processes();
            let ctx = Ctx { admin, procs: &procs, progress: &prog, cancel: &cancel };
            let rep = clean::clean(&defs, &results, &env, &ctx);
            let _ = tx.send(Msg::Cleaned(rep));
        });
    }

    // ───────────────────────────── marco de la ventana ─────────────────────────────

    /// jimy-betanzos · Dibuja la barra de título con los botones de ventana.
    fn titlebar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("titlebar")
            .exact_height(40.0)
            .show_separator_line(false)
            .frame(egui::Frame::none().fill(alpha(BG, 210)).inner_margin(Margin::symmetric(16.0, 0.0)))
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
                    ui.label(RichText::new("NEXO").font(font(13.0)).color(ACCENT));
                    ui.label(RichText::new("Control local del sistema").font(font(12.0)).color(MUTED));
                    ui.add_space(14.0);
                    let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 3.5, ACCENT);
                    ui.painter().circle_filled(r.center(), 7.0, alpha(ACCENT, 40));
                    ui.label(RichText::new("Sin conexión · 100 % local").font(font(11.0)).color(alpha(ACCENT, 220)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if title_btn(ui, "×", DANGER).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                        if title_btn(ui, if maximized { "❐" } else { "☐" }, ACCENT).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                        }
                        if title_btn(ui, "—", ACCENT).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                        }
                    });
                });
            });
    }

    /// jimy-betanzos · Dibuja el menú lateral.
    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("side")
            .exact_width(240.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::none()
                    .fill(alpha(PANEL, 175))
                    .stroke(Stroke::new(1.0_f32, alpha(ACCENT, 28)))
                    .inner_margin(Margin { left: 14.0, right: 14.0, top: 14.0, bottom: 14.0 }),
            )
            .show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::hover());
                let p = ui.painter();
                logo(p, r.left_center() + vec2(28.0, 0.0), 21.0);
                p.text(r.left_center() + vec2(62.0, -9.0), Align2::LEFT_CENTER, "NEXO", font(27.0), TEXT);
                p.text(r.left_center() + vec2(63.0, 17.0), Align2::LEFT_CENTER, "Optimización inteligente", font(9.5), ACCENT);
                ui.add_space(12.0);

                let pages: [(Page, &str, &str, Option<&str>); 8] = [
                    (Page::Panel, "🏠", "Panel", None),
                    (Page::Clean, "🗑", "Limpieza", None),
                    (Page::Auto, "⏰", "Automático", None),
                    (Page::History, "📜", "Historial", None),
                    (Page::Ram, "📊", "Memoria RAM", None),
                    (Page::Diag, "🔍", "Diagnóstico", None),
                    (Page::Shield, "🔒", "Protección", None),
                    (Page::Mobile, "📱", "Celulares", None),
                ];
                for (pg, icon, label, tag) in pages {
                    if nav_item(ui, icon, label, self.page == pg, tag).clicked() {
                        self.page = pg;
                        if pg == Page::Auto {
                            self.check_task();
                        }
                    }
                }

                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"), " · sin conexión a internet")).size(11.0).color(MUTED));
                    let (txt, col) = if cfg!(unix) { ("• Modo usuario", ACCENT) } else if self.admin { ("• Acceso total", ACCENT) } else { ("• Acceso limitado", WARN) };
                    tinted_card(col).inner_margin(Margin::symmetric(12.0, 8.0)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(txt).font(font(12.0)).color(col));
                    });
                });
            });
    }

    // ───────────────────────────── páginas ─────────────────────────────

    /// jimy-betanzos · Pantalla Panel: estado del equipo.
    fn page_panel(&mut self, ui: &mut Ui) {
        header(ui, "› Centro de control", &self.model, "Estado de tu equipo en tiempo real. Todo se analiza aquí mismo, sin internet.");
        let s = &self.snap;
        let disk_used = 1.0 - s.disk_free as f32 / s.disk_total.max(1) as f32;
        let mem = s.mem.load as f32 / 100.0;
        let (disk_free, disk_total) = (s.disk_free, s.disk_total);
        let (mem_load, mem_avail, mem_total) = (s.mem.load, s.mem.avail, s.mem.total);
        let (on_ac, pct, has_bat) = (s.power.on_ac, s.power.percent, s.power.has_battery);
        let auto_on = self.cfg.auto_enabled && self.task_registered;
        let auto_sub = format!("{} · {:02}:00", self.cfg.frequency.label(), self.cfg.hour);
        let last = self.history.iter().rev().find(|h| h.mode != "pospuesta").cloned();
        let health = self.diag.as_ref().map(|d| (d.overall, d.ts));
        let idle = self.busy.is_none();
        let mut scan_now = false;

        ui.columns(4, |c| {
            stat_card(&mut c[0], "💾", "Disco del sistema", &format!("{} libres", fmt::bytes(disk_free)),
                &format!("de {} · {:.0} % usado", fmt::bytes(disk_total), disk_used * 100.0), level_color(disk_used), Some(disk_used));
            stat_card(&mut c[1], "📊", "Memoria RAM", &format!("{} %", mem_load),
                &format!("{} libres de {}", fmt::bytes(mem_avail), fmt::bytes(mem_total)), level_color(mem), Some(mem));
            let bat = pct.unwrap_or(100) as f32 / 100.0;
            let bcol = if bat < 0.3 && !on_ac { WARN } else { ACCENT };
            stat_card(&mut c[2], "🔋", "Energía", if on_ac { "Enchufado" } else { "Con batería" },
                &if has_bat { format!("{} % de carga", pct.unwrap_or(0)) } else { "Sin batería detectada".into() }, bcol, has_bat.then_some(bat));
            stat_card(&mut c[3], "⏰", "Modo automático", if auto_on { "Activo" } else { "Apagado" },
                &if auto_on { auto_sub.clone() } else { "Configúralo en Automático".into() }, if auto_on { ACCENT } else { MUTED }, None);
        });
        ui.add_space(10.0);

        ui.columns(2, |c| {
            card().show(&mut c[0], |ui| {
                ui.set_width(ui.available_width());
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 250.0), Sense::hover());
                let col = level_color(disk_used);
                ring(ui.painter(), r.center(), 88.0, disk_used, col, 10.0);
                ui.painter().text(r.center() - vec2(0.0, 10.0), Align2::CENTER_CENTER, format!("{:.0}%", disk_used * 100.0), font(44.0), TEXT);
                ui.painter().text(r.center() + vec2(0.0, 26.0), Align2::CENTER_CENTER, "Disco usado", font(12.0), col);
                ui.vertical_centered(|ui| {
                    if disk_used >= 0.75 {
                        ui.label(RichText::new("Tu disco está por encima del 75 %. Una limpieza puede ayudar.").size(13.0).color(WARN));
                    } else {
                        ui.label(RichText::new("Espacio en buen estado.").size(13.0).color(MUTED));
                    }
                    ui.add_space(6.0);
                    if button(ui, "🔍  Escanear ahora", ACCENT, vec2(ui.available_width().min(340.0), 48.0), idle).clicked() {
                        scan_now = true;
                    }
                });
            });
            let ui = &mut c[1];
            tinted_card(ACCENT).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("🛡  Garantías de NEXO").font(font(15.0)).color(ACCENT));
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
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Última actividad").font(font(12.0)).color(MUTED));
                match &last {
                    Some(h) => {
                        ui.label(RichText::new(format!("{} liberados", fmt::bytes(h.freed))).font(font(24.0)).color(ACCENT));
                        ui.label(RichText::new(format!(
                            "Limpieza {} · {} · {} archivos",
                            if h.mode == "auto" { "automática" } else { "manual" },
                            fmt::ago(h.ts),
                            fmt::count(h.files)
                        )).size(12.5).color(MUTED));
                    }
                    None => {
                        ui.label(RichText::new("Aún no hay limpiezas registradas.").size(13.5).color(MUTED));
                    }
                }
                ui.add_space(6.0);
                ui.label(RichText::new("Salud del equipo").font(font(12.0)).color(MUTED));
                match health {
                    Some((score, ts)) => {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{}/100", score)).font(font(24.0)).color(score_color(score)));
                            ui.label(RichText::new(format!("{} · {}", diag::grade(score), fmt::ago(ts))).size(12.5).color(MUTED));
                        });
                    }
                    None => {
                        ui.label(RichText::new("Sin diagnóstico todavía: hazlo en la pestaña Diagnóstico.").size(13.5).color(MUTED));
                    }
                }
            });
        });

        if scan_now {
            self.page = Page::Clean;
            self.spawn_scan(None, false);
        }
    }

    /// jimy-betanzos · Aviso cuando falta el permiso de administrador.
    fn admin_banner(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.admin || cfg!(unix) {
            return;
        }
        tinted_card(WARN).inner_margin(Margin::symmetric(16.0, 10.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("⚡").color(WARN).size(16.0));
                ui.label(RichText::new("Rechazaste el permiso de Windows: se omiten Windows Update, temporales de Windows y registros del sistema.").size(13.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if link(ui, "Volver a pedir permiso", WARN).clicked() && sys::relaunch_as_admin() {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                });
            });
        });
        ui.add_space(6.0);
    }

    /// jimy-betanzos · Pantalla Limpieza.
    fn page_clean(&mut self, ui: &mut Ui, t: f32) {
        header(ui, "› Limpieza inteligente", "Buscar y eliminar basura", "Nada se borra sin que lo veas primero. Tus videos y documentos quedan protegidos.");
        let ctx = ui.ctx().clone();
        self.admin_banner(ui, &ctx);
        match self.busy {
            Some(Busy::Scan) | Some(Busy::Clean) => self.busy_view(ui, t),
            _ if self.results.is_some() => self.results_view(ui),
            _ => self.idle_view(ui),
        }
    }

    /// jimy-betanzos · Vista mientras se escanea o limpia.
    fn busy_view(&mut self, ui: &mut Ui, t: f32) {
        let p = self.progress.lock().map(|p| p.clone()).unwrap_or_default();
        let cleaning = self.busy == Some(Busy::Clean);
        let col = ACCENT;
        let mut cancel = false;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 300.0), Sense::hover());
            scanner(ui.painter(), r.center(), 112.0, t, col);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(if cleaning { "Limpiando…" } else { "Escaneando…" }).font(font(26.0)).color(col));
                let frac = if cleaning && p.total_bytes > 0 {
                    (p.bytes as f32 / p.total_bytes as f32).min(0.99)
                } else if p.steps > 0 {
                    (p.step as f32 / p.steps as f32).min(0.99)
                } else {
                    0.0
                };
                ui.label(RichText::new(format!("{:.0} %", frac * 100.0)).font(font(34.0)).color(TEXT));
                ui.label(RichText::new(p.label.clone()).size(14.0));
                ui.add_space(4.0);
                ui.allocate_ui(vec2(460.0, 12.0), |ui| progress_bar(ui, Some(frac), col, t));
                let elapsed = (self.now - self.busy_started).max(0.0);
                let eta = if frac > 0.04 && elapsed > 1.0 { format!(" · quedan ≈ {}", fmt::secs(elapsed * (1.0 - frac as f64) / frac as f64)) } else { String::new() };
                ui.label(RichText::new(format!("{} transcurridos{}", fmt::secs(elapsed), eta)).size(12.5).color(MUTED));
                let line = if cleaning {
                    format!("{} archivos eliminados · {} liberados", fmt::count(p.files), fmt::bytes(p.bytes))
                } else {
                    format!("{} archivos analizados", fmt::count(p.files))
                };
                ui.label(RichText::new(line).size(13.0).color(MUTED));
                ui.label(RichText::new(fmt::short_path(Path::new(&p.current), 96)).monospace().size(11.0).color(alpha(MUTED, 170)));
                ui.add_space(8.0);
                if button(ui, "Cancelar", DANGER, vec2(170.0, 38.0), true).clicked() {
                    cancel = true;
                }
            });
        });
        if cancel {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// jimy-betanzos · Vista inicial de Limpieza («Listo para escanear»).
    fn idle_view(&mut self, ui: &mut Ui) {
        if let Some(rep) = &self.report {
            tinted_card(ACCENT).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("✔  Limpieza completada").font(font(18.0)).color(ACCENT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(fmt::bytes(rep.freed)).font(font(30.0)).color(ACCENT));
                    });
                });
                ui.label(format!("{} archivos eliminados o movidos.", fmt::count(rep.files)));
                if rep.recycled > 0 {
                    ui.label(RichText::new(format!("♻ {} elementos enviados a la Papelera: puedes recuperarlos desde allí.", fmt::count(rep.recycled))).color(WARN));
                }
                if rep.skipped > 0 {
                    ui.label(RichText::new(format!("{} archivos estaban en uso y se dejaron intactos.", fmt::count(rep.skipped))).color(MUTED));
                }
                ui.horizontal_wrapped(|ui| {
                    for (n, b) in rep.per_cat.iter().filter(|(_, b)| *b > 0) {
                        badge(ui, &format!("{} · {}", n, fmt::bytes(*b)), ACCENT);
                    }
                });
            });
            ui.add_space(8.0);
        }
        let mut go = false;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 260.0), Sense::hover());
            scanner(ui.painter(), r.center(), 100.0, 0.8, alpha(ACCENT, 170));
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Listo para escanear").font(font(22.0)));
                ui.label(RichText::new("El escaneo solo mira: no borra nada. Después tú eliges qué limpiar.").size(13.5).color(MUTED));
                ui.add_space(8.0);
                if button(ui, "🔍  Iniciar escaneo", ACCENT, vec2(300.0, 50.0), true).clicked() {
                    go = true;
                }
            });
        });
        if go {
            self.spawn_scan(None, false);
        }
    }

    /// jimy-betanzos · Vista con los resultados del escaneo por categoría.
    fn results_view(&mut self, ui: &mut Ui) {
        let admin = self.admin;
        let mut open_confirm = false;
        let mut rescan = false;
        let Some(results) = self.results.as_mut() else { return };

        let safe: u64 = results.iter().filter(|r| r.kind == Kind::Safe).map(|r| r.size).sum();
        let safe_n: u64 = results.iter().filter(|r| r.kind == Kind::Safe).map(|r| r.count).sum();
        let review: u64 = results.iter().filter(|r| r.kind == Kind::Review).map(|r| r.size).sum();
        let (prot, prot_n) = results
            .iter()
            .find(|r| r.kind == Kind::Protected)
            .map(|r| (r.size, r.count))
            .unwrap_or((0, 0));
        // Resumen compacto en una sola franja: deja más altura para revisar archivos.
        card().inner_margin(Margin::symmetric(18.0, 10.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.columns(3, |c| {
                for (k, (title, value, sub, col)) in [
                    ("Basura eliminable", fmt::bytes(safe), format!("{} temporales y caché", fmt::count(safe_n)), ACCENT),
                    ("Para revisar", fmt::bytes(review), "Papelera, capturas, instaladores".to_string(), WARN),
                    ("Videos y documentos", fmt::bytes(prot), format!("{} protegidos", fmt::count(prot_n)), ACCENT),
                ]
                .into_iter()
                .enumerate()
                {
                    c[k].horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(4.0, 40.0), Sense::hover());
                        ui.painter().rect_filled(r, 0.0, col);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(title).font(font(11.0)).color(col));
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(value).font(font(22.0)).color(TEXT));
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
                (Kind::Safe, "Limpieza segura", "basura del sistema · se elimina definitivamente", ACCENT),
                (Kind::Review, "Para revisar", "van a la Papelera de reciclaje · recuperables", WARN),
                (Kind::Protected, "Protegidos", "videos y documentos · nunca en automático · a la Papelera", ACCENT),
            ] {
                section(ui, title, sub, col);
                for r in results.iter_mut().filter(|r| r.kind == kind) {
                    category_card(ui, r, admin, thumbs);
                }
            }
        });

        let sel: u64 = results.iter().map(|r| r.selected_size()).sum();
        let sel_cats = results.iter().filter(|r| r.selected_size() > 0).count();
        card().inner_margin(Margin::symmetric(18.0, 12.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Seleccionado").font(font(11.0)).color(MUTED));
                    ui.label(RichText::new(fmt::bytes(sel)).font(font(26.0)).color(if sel > 0 { ACCENT } else { MUTED }));
                });
                ui.label(RichText::new(format!("en {} categorías", sel_cats)).color(MUTED));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if button(ui, "🗑  Limpiar selección", ACCENT, vec2(250.0, 44.0), sel > 0).clicked() {
                        open_confirm = true;
                    }
                    if button(ui, "⟳  Re-escanear", ACCENT, vec2(170.0, 44.0), true).clicked() {
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
            self.spawn_scan(None, false);
        }
    }

    /// jimy-betanzos · Pantalla Automático: programación de la limpieza.
    fn page_auto(&mut self, ui: &mut Ui, t: f32) {
        header(ui, "› Piloto automático", "Limpieza automática", &format!("{} abre NEXO a la hora elegida, limpia la basura y lo cierra. En reposo consume 0 MB de RAM.", diag::txt("os")));
        let mut apply = false;
        let mut simulate = false;
        let idle = self.busy.is_none() && self.pending_actions == 0;
        let simulating = self.busy == Some(Busy::Simulate);
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
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        toggle(ui, &mut cfg.auto_enabled);
                        ui.label(RichText::new("Limpieza automática").font(font(19.0)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if registered {
                                badge(ui, "Programada", ACCENT);
                            } else {
                                badge(ui, "No programada", MUTED);
                            }
                        });
                    });
                    ui.add_space(10.0);
                    ui.label(RichText::new("Frecuencia").font(font(12.0)).color(MUTED));
                    ui.horizontal(|ui| {
                        for f in [Frequency::Daily, Frequency::Weekly, Frequency::Monthly] {
                            if chip(ui, f.label(), cfg.frequency == f).clicked() {
                                cfg.frequency = f;
                            }
                        }
                    });
                    ui.label(RichText::new(match cfg.frequency {
                        Frequency::Daily => "Todos los días.",
                        Frequency::Weekly => "Cada domingo.",
                        Frequency::Monthly => "El día 1 de cada mes.",
                    }).size(12.5).color(MUTED));
                    ui.add_space(6.0);
                    ui.label(RichText::new("Hora").font(font(12.0)).color(MUTED));
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
                    ui.label(RichText::new("Qué limpia · solo basura del sistema").font(font(12.0)).color(MUTED));
                    for d in rules::auto_eligible(&defs) {
                        let mut on = cfg.auto_categories.iter().any(|x| x == d.id);
                        let label = if d.admin_only() && !admin {
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
                        if button(ui, "💾  Guardar y aplicar", ACCENT, vec2(220.0, 44.0), idle).clicked() {
                            apply = true;
                        }
                        if button(ui, "▶  Simular ahora", ACCENT, vec2(190.0, 44.0), idle).clicked() {
                            simulate = true;
                        }
                    });
                });
                });

                // Columna sin justificar: evita que el texto se estire palabra a palabra.
                c[1].with_layout(Layout::top_down(Align::Min), |ui| {
                tinted_card(ACCENT).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("🛡  Tus archivos están a salvo").font(font(15.0)).color(ACCENT));
                    ui.label("En automático NUNCA se tocan videos, documentos, capturas ni descargas.");
                    ui.label(RichText::new("Si encuentra algo así, solo te avisa con una notificación para que tú decidas en la pestaña Limpieza.").size(13.0).color(MUTED));
                });
                ui.add_space(8.0);
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Condiciones inteligentes").font(font(13.0)).color(ACCENT));
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
                    card().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 120.0), Sense::hover());
                        scanner(ui.painter(), r.center(), 46.0, t, ACCENT);
                        ui.vertical_centered(|ui| ui.label(RichText::new("Simulando limpieza automática…").color(ACCENT)));
                    });
                } else if let Some((freed, files, per)) = sim {
                    tinted_card(ACCENT).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("Resultado de la simulación (no se borró nada)").font(font(12.0)).color(ACCENT));
                        ui.label(RichText::new(format!("Liberaría {}", fmt::bytes(*freed))).font(font(26.0)));
                        ui.label(RichText::new(format!("{} archivos de basura · 0 videos o documentos", fmt::count(*files))).color(MUTED));
                        ui.horizontal_wrapped(|ui| {
                            for (n, b) in per {
                                badge(ui, &format!("{} · {}", n, fmt::bytes(*b)), ACCENT);
                            }
                        });
                    });
                }
                if let Some(h) = &last_auto {
                    ui.add_space(8.0);
                    card().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("Última ejecución automática").font(font(12.0)).color(MUTED));
                        if h.mode == "pospuesta" {
                            ui.label(RichText::new(&h.note).color(WARN));
                        } else {
                            ui.label(RichText::new(format!("{} liberados · {}", fmt::bytes(h.freed), fmt::ago(h.ts))).font(font(18.0)).color(ACCENT));
                        }
                        ui.label(RichText::new(fmt::date(h.ts)).size(12.0).color(MUTED));
                    });
                }
                ui.add_space(8.0);
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Quitar NEXO de Windows").font(font(12.0)).color(MUTED));
                    ui.label(RichText::new("Borra la tarea automática, el vigía, el acceso directo y la copia instalada. Tu historial se conserva.").size(12.5).color(MUTED));
                    let label = if armed { "¿Seguro? Pulsa otra vez para quitarlo" } else { "Quitar NEXO del sistema" };
                    if link(ui, label, if armed { DANGER } else { WARN }).clicked() {
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
                self.show_toast("Quitando NEXO del sistema…".into(), ACCENT, now);
                self.bg_action("Quitando NEXO del sistema", move |_progress| {
                    let done = install::uninstall();
                    let reg = schedule::is_registered();
                    Box::new(move |a: &mut NexoApp, now| {
                        a.task_registered = reg;
                        a.cfg.auto_enabled = false;
                        a.cfg.vigia_enabled = false;
                        a.cfg.save();
                        a.vigia_running = false;
                        a.show_toast(format!("NEXO quitado del sistema · {}", done.join(" · ")), WARN, now);
                    })
                });
            } else {
                self.uninstall_armed = true;
            }
        }
        if apply {
            self.cfg.save();
            let (cfg, admin) = (self.cfg.clone(), self.admin);
            self.show_toast("Aplicando…".into(), ACCENT, now);
            self.bg_action("Programando la limpieza automática", move |_progress| {
                let res = if cfg.auto_enabled { schedule::enable(&cfg, admin) } else { schedule::disable() };
                let reg = schedule::is_registered();
                Box::new(move |a: &mut NexoApp, now| {
                    a.task_registered = reg;
                    match res {
                        Ok(()) if cfg.auto_enabled => {
                            let msg = format!("Modo automático activado · {} a las {:02}:00", cfg.frequency.label(), cfg.hour);
                            a.show_toast(msg, ACCENT, now);
                        }
                        Ok(()) => a.show_toast("Modo automático desactivado".into(), WARN, now),
                        Err(e) => a.show_toast(format!("No se pudo programar la tarea: {}", e), DANGER, now),
                    }
                })
            });
        }
        if simulate {
            self.sim = None;
            let ids = self.cfg.auto_categories.clone();
            self.spawn_scan(Some(ids), true);
        }
    }

    /// jimy-betanzos · Pantalla Historial de limpiezas.
    fn page_history(&mut self, ui: &mut Ui) {
        header(ui, "› Registro", "Historial de limpiezas", "Cada limpieza, manual o automática, queda anotada aquí.");
        if self.history.is_empty() {
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Aún no hay limpiezas registradas.").font(font(18.0)));
                ui.label(RichText::new("Haz tu primer escaneo en la pestaña Limpieza.").color(MUTED));
            });
            return;
        }
        let total: u64 = self.history.iter().map(|h| h.freed).sum();
        ui.label(RichText::new(format!("Total liberado con NEXO: {}", fmt::bytes(total))).font(font(16.0)).color(ACCENT));
        ui.add_space(4.0);
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for h in self.history.iter().rev() {
                card().inner_margin(Margin::symmetric(18.0, 12.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        match h.mode.as_str() {
                            "auto" => badge(ui, "Automática", ACCENT),
                            "pospuesta" => badge(ui, "Pospuesta", WARN),
                            _ => badge(ui, "Manual", ACCENT),
                        }
                        ui.label(RichText::new(fmt::date(h.ts)).font(font(14.0)));
                        ui.label(RichText::new(fmt::ago(h.ts)).size(12.0).color(MUTED));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if h.mode != "pospuesta" {
                                ui.label(RichText::new(fmt::bytes(h.freed)).font(font(22.0)).color(ACCENT));
                            }
                        });
                    });
                    if !h.note.is_empty() {
                        ui.label(RichText::new(&h.note).color(WARN));
                    }
                    if h.mode != "pospuesta" {
                        ui.label(RichText::new(format!(
                            "{} archivos · {} a la Papelera · {} en uso (intactos)",
                            fmt::count(h.files),
                            fmt::count(h.recycled),
                            fmt::count(h.skipped)
                        )).size(12.5).color(MUTED));
                    }
                    if h.review_bytes > 0 {
                        ui.label(RichText::new(format!("Quedó para revisar: {} (capturas, descargas, videos y documentos)", fmt::bytes(h.review_bytes))).size(12.5).color(ACCENT));
                    }
                    ui.horizontal_wrapped(|ui| {
                        for (n, b) in h.details.iter().filter(|(_, b)| *b > 0) {
                            badge(ui, &format!("{} · {}", n, fmt::bytes(*b)), ACCENT);
                        }
                    });
                });
                ui.add_space(4.0);
            }
        });
    }

    /// jimy-betanzos · Ventana para confirmar la limpieza.
    fn confirm_modal(&mut self, ctx: &egui::Context) {
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

        let chosen: Vec<&CatResult> = results.iter().filter(|r| r.selected_size() > 0).collect();
        let (prot_n, prot_b) = chosen
            .iter()
            .filter(|r| r.kind == Kind::Protected)
            .map(|r| (r.selected_count(), r.selected_size()))
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        let empties_bin = chosen.iter().any(|r| r.special == Special::RecycleBin);
        let total: u64 = chosen.iter().map(|r| r.selected_size()).sum();
        let mut close = false;
        let mut go = false;
        let ack = &mut self.ack_protected;

        let area = egui::Area::new(egui::Id::new("confirm"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                card().fill(PANEL).stroke(Stroke::new(1.5_f32, alpha(ACCENT, 150))).inner_margin(Margin::same(24.0)).show(ui, |ui| {
                    ui.set_width(580.0);
                    ui.label(RichText::new("› Confirmar limpieza").font(font(12.0)).color(ACCENT));
                    ui.label(RichText::new("Esto es lo que se va a hacer").font(font(24.0)));
                    ui.add_space(6.0);
                    ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for r in &chosen {
                            let (col, dest) = match (r.kind, r.special) {
                                (_, Special::RecycleBin) => (WARN, "Se vacía la Papelera (definitivo)".to_string()),
                                (Kind::Safe, _) => (ACCENT, "Eliminación definitiva · basura del sistema".to_string()),
                                (Kind::Protected, _) => (ACCENT, format!("{} elementos → Papelera (recuperables)", r.selected_count())),
                                _ => (WARN, format!("{} elementos → Papelera (recuperables)", r.selected_count())),
                            };
                            ui.horizontal(|ui| {
                                let (d, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                                ui.painter().circle_filled(d.center(), 4.0, col);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(r.name).font(font(15.0)));
                                    ui.label(RichText::new(dest).size(12.0).color(MUTED));
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(RichText::new(fmt::bytes(r.selected_size())).font(font(15.0)).color(col));
                                });
                            });
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Total").font(font(13.0)).color(MUTED));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(fmt::bytes(total)).font(font(22.0)).color(ACCENT));
                        });
                    });
                    if prot_n > 0 {
                        tinted_card(ACCENT).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(format!("⚠  Incluye {} videos/documentos ({}).", prot_n, fmt::bytes(prot_b))).color(ACCENT).font(font(14.0)));
                            ui.label(RichText::new("Irán a la Papelera de reciclaje: podrás recuperarlos desde allí.").size(13.0));
                            ui.checkbox(ack, "Sí, quiero botar estos videos y documentos");
                        });
                    }
                    if empties_bin {
                        tinted_card(WARN).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new("⚠  Vaciar la Papelera borra su contenido para siempre.").color(WARN));
                        });
                    }
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if button(ui, "✔  Confirmar", ACCENT, vec2(190.0, 44.0), prot_n == 0 || *ack).clicked() {
                            go = true;
                        }
                        if button(ui, "Cancelar", MUTED, vec2(150.0, 44.0), true).clicked() {
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
            self.spawn_clean();
        } else if close {
            self.confirm = false;
        }
    }

    // ───────────────────────────── diagnóstico y protección (fase 3) ─────────────────────────────

    /// jimy-betanzos · Recibe los resultados del diagnóstico y del antivirus.
    fn poll_phase3(&mut self, now: f64) {
        if let Some(r) = self.diag_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.diag_rx = None;
            self.trim_in = 3;
            match r {
                Ok(rep) => {
                    let s = rep.overall;
                    self.diag = Some(rep);
                    self.show_toast(format!("Diagnóstico listo · {}/100 · {}", s, diag::grade(s)), score_color(s), now);
                }
                Err(e) => self.show_toast(format!("No se pudo completar el diagnóstico: {}", e), DANGER, now),
            }
        }
        if let Some((r, secs)) = self.mpscan_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.mpscan_rx = None;
            if r.is_ok() && secs > 5 {
                // Se recuerda cuánto tardó para estimar mejor la próxima barra.
                self.cfg.defender_scan_secs = secs;
                self.cfg.save();
            }
            let (msg, col) = match r {
                Ok(0) => ("✔ Análisis rápido terminado: no se encontraron amenazas.".to_string(), ACCENT),
                Ok(2) => (format!("⚠ {} encontró amenazas. Revisa {}.", diag::txt("av"), diag::txt("security_app")), DANGER),
                Ok(c) => (format!("El análisis terminó con el código {}. Revisa {}.", c, diag::txt("security_app")), WARN),
                Err(e) => (format!("No se pudo lanzar el análisis: {}", e), DANGER),
            };
            notify::toast(&format!("Análisis de {}", diag::txt("av")), &msg, "");
            if col == ACCENT {
                if let Some(d) = self.diag.as_mut() {
                    d.raw.defender.quick_age = 0;
                    diag::analyze(d);
                    diag::save(d);
                }
            }
            self.mpscan_msg = Some((msg, col));
        }
    }

    /// jimy-betanzos · Lanza el diagnóstico en un hilo aparte.
    fn start_diag(&mut self) {
        if self.diag_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        self.diag_rx = Some(rx);
        self.diag_started = self.now;
        let prog = self.diag_prog.clone();
        prog.store(0, std::sync::atomic::Ordering::Relaxed);
        std::thread::spawn(move || {
            sys::interactive_thread();
            let _ = tx.send(diag::run_with_progress(&prog));
        });
    }

    /// jimy-betanzos · Lanza el análisis rápido del antivirus.
    fn start_mpscan(&mut self) {
        if self.mpscan_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        self.mpscan_rx = Some(rx);
        self.mpscan_msg = None;
        self.mp_started = self.now;
        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            let r = diag::quick_scan();
            let _ = tx.send((r, start.elapsed().as_secs()));
        });
    }

    /// jimy-betanzos · Ejecuta la acción de una recomendación.
    fn do_action(&mut self, a: Action, now: f64) {
        match a {
            Action::GoClean => self.page = Page::Clean,
            Action::GoRam => self.page = Page::Ram,
            Action::QuickScan => {
                self.start_mpscan();
                self.page = Page::Shield;
            }
            Action::SetBalanced => self.set_balanced(true, now),
            Action::BatteryReport => {
                std::thread::spawn(diag::battery_report);
                self.show_toast("Abriendo el informe de batería…".into(), ACCENT, now);
            }
            Action::Open(t) => diag::open(&t),
        }
    }

    /// jimy-betanzos · Cambia a «Equilibrado» guardando el plan anterior, o lo restaura (deshacer).
    fn set_balanced(&mut self, on: bool, now: f64) {
        if self.pending_actions > 0 {
            return;
        }
        let prev = self.cfg.prev_power_plan.clone();
        if !on && prev.is_none() {
            return;
        }
        self.show_toast("Cambiando el plan de energía…".into(), ACCENT, now);
        self.bg_action("Cambiando el plan de energía", move |_progress| {
            let (cur, _) = diag::active_power_plan();
            let target = if on { diag::BALANCED.to_string() } else { prev.unwrap_or_default() };
            let ok = diag::set_power_plan(&target);
            let (g, n) = diag::active_power_plan();
            Box::new(move |a: &mut NexoApp, now| {
                if !ok {
                    a.show_toast("No se pudo cambiar el plan de energía.".into(), DANGER, now);
                    return;
                }
                if on && cur != diag::BALANCED {
                    a.cfg.prev_power_plan = Some(cur);
                }
                if !on {
                    a.cfg.prev_power_plan = None;
                }
                a.cfg.save();
                if let Some(d) = a.diag.as_mut() {
                    d.power_guid = g;
                    d.power_name = n.clone();
                    diag::analyze(d);
                    diag::save(d);
                }
                a.show_toast(format!("Plan de energía: {}", n), ACCENT, now);
            })
        });
    }

    /// jimy-betanzos · Activa o desactiva un programa de inicio.
    fn toggle_startup(&mut self, i: usize, on: bool, now: f64) {
        let result = {
            let Some(d) = self.diag.as_mut() else { return };
            let Some(item) = d.raw.startup.get(i).cloned() else { return };
            let r = diag::set_startup_enabled(&item, on);
            if r.is_ok() {
                d.raw.startup[i].enabled = on;
                diag::analyze(d);
                diag::save(d);
            }
            r.map(|_| item.name)
        };
        match result {
            Ok(name) => self.show_toast(
                format!("{} {} al encender {}", name, if on { "se abrirá" } else { "ya no se abrirá" }, diag::txt("os")),
                ACCENT,
                now,
            ),
            Err(e) => self.show_toast(e, DANGER, now),
        }
    }

    /// jimy-betanzos · Vista mientras corre el diagnóstico.
    fn diag_running_view(&mut self, ui: &mut Ui, t: f32) {
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 280.0), Sense::hover());
            scanner(ui.painter(), r.center(), 104.0, t, ACCENT);
            ui.vertical_centered(|ui| {
                let f = self.diag_prog.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
                ui.label(RichText::new("Analizando tu equipo…").font(font(24.0)).color(ACCENT));
                ui.label(RichText::new(format!("{:.0} %", f * 100.0)).font(font(30.0)).color(TEXT));
                ui.allocate_ui(vec2(460.0, 12.0), |ui| progress_bar(ui, Some(f), ACCENT, t));
                ui.label(RichText::new("Disco · batería · antivirus · firewall · actualizaciones · programas de inicio · tareas programadas").size(13.0));
                ui.label(RichText::new(format!("{} transcurridos · solo lee: no cambia nada", fmt::secs((self.now - self.diag_started).max(0.0)))).size(12.5).color(MUTED));
            });
        });
    }

    /// jimy-betanzos · Tarjeta inicial del diagnóstico con su botón.
    fn diag_hero(&mut self, ui: &mut Ui, title: &str, sub: &str, action: &str) {
        let mut go = false;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 240.0), Sense::hover());
            scanner(ui.painter(), r.center(), 92.0, 0.8, alpha(ACCENT, 170));
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(title).font(font(22.0)));
                ui.label(RichText::new(sub).size(13.5).color(MUTED));
                ui.add_space(8.0);
                if button(ui, action, ACCENT, vec2(320.0, 50.0), true).clicked() {
                    go = true;
                }
            });
        });
        if go {
            self.start_diag();
        }
    }

    /// jimy-betanzos · Pantalla Diagnóstico.
    fn page_diag(&mut self, ui: &mut Ui, t: f32) {
        header(ui, "› Diagnóstico", "Chequeo general", "Una nota de 0 a 100 por áreas, calculada aquí mismo y sin internet. Las mejoras se pueden deshacer.");
        let now = ui.input(|i| i.time);
        if self.diag_rx.is_some() {
            self.diag_running_view(ui, t);
            return;
        }
        let Some(rep) = self.diag.clone() else {
            self.diag_hero(ui, "Revisa la salud de tu equipo", "Disco, batería, memoria, seguridad y programas de inicio. Solo lee: no cambia nada.", "🔍  Iniciar diagnóstico");
            return;
        };
        let admin = self.admin;
        let can_undo = self.cfg.prev_power_plan.is_some();
        let mut action: Option<Action> = None;
        let mut toggle_item: Option<(usize, bool)> = None;
        let mut rerun = false;
        let mut undo_power = false;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |c| {
                card().show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(200.0, 196.0), Sense::hover());
                        let col = score_color(rep.overall);
                        ring(ui.painter(), r.center(), 72.0, rep.overall as f32 / 100.0, col, 10.0);
                        ui.painter().text(r.center() - vec2(0.0, 8.0), Align2::CENTER_CENTER, rep.overall.to_string(), font(46.0), TEXT);
                        ui.painter().text(r.center() + vec2(0.0, 26.0), Align2::CENTER_CENTER, "DE 100", font(11.0), col);
                        ui.vertical(|ui| {
                            ui.add_space(24.0);
                            ui.label(RichText::new(diag::grade(rep.overall)).font(font(28.0)).color(col));
                            ui.label(RichText::new(format!("Diagnóstico {}", fmt::ago(rep.ts))).size(12.5).color(MUTED));
                            let n2 = rep.recs.iter().filter(|r| r.severity == 2).count();
                            let n1 = rep.recs.iter().filter(|r| r.severity == 1).count();
                            ui.label(RichText::new(format!("{} importantes · {} para revisar", n2, n1)).size(13.0));
                            ui.add_space(8.0);
                            if button(ui, "⟳  Repetir", ACCENT, vec2(160.0, 40.0), true).clicked() {
                                rerun = true;
                            }
                        });
                    });
                });
                card().show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Notas por área").font(font(12.0)).color(MUTED));
                    let w = ui.available_width() / rep.areas.len().max(1) as f32;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (name, score) in &rep.areas {
                            let (r, _) = ui.allocate_exact_size(vec2(w, 170.0), Sense::hover());
                            let c0 = r.center() - vec2(0.0, 12.0);
                            let p = ui.painter();
                            match score {
                                Some(s) => {
                                    let col = score_color(*s);
                                    ring(p, c0, 26.0, *s as f32 / 100.0, col, 5.0);
                                    p.text(c0, Align2::CENTER_CENTER, s.to_string(), font(20.0), TEXT);
                                }
                                None => {
                                    ring(p, c0, 26.0, 0.0, MUTED, 5.0);
                                    p.text(c0, Align2::CENTER_CENTER, "—", font(20.0), MUTED);
                                }
                            }
                            let label = if name == "Almacenamiento" { "Disco" } else { name.as_str() };
                            p.text(pos2(r.center().x, r.bottom() - 22.0), Align2::CENTER_CENTER, label, font(12.5), TEXT);
                        }
                    });
                });
            });

            section(ui, "Recomendaciones", "ordenadas por importancia", ACCENT);
            if rep.recs.is_empty() {
                tinted_card(ACCENT).show(ui, |ui| {
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
                card().stroke(Stroke::new(1.0_f32, alpha(col, 90))).inner_margin(Margin::symmetric(16.0, 10.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() - 190.0).max(260.0);
                        ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::LEFT), |ui| {
                            ui.horizontal(|ui| {
                                badge(ui, tag, col);
                                badge(ui, &r.area, MUTED);
                                ui.label(RichText::new(&r.title).font(font(15.0)));
                            });
                            ui.label(RichText::new(&r.detail).size(12.5).color(MUTED));
                        });
                        if let Some(a) = &r.action {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if button(ui, a.label(), col, vec2(170.0, 36.0), true).clicked() {
                                    action = Some(a.clone());
                                }
                            });
                        }
                    });
                });
                ui.add_space(2.0);
            }

            section(ui, "Programas de inicio", "desactivarlos no los desinstala · se pueden volver a activar", ACCENT);
            card().inner_margin(Margin::symmetric(16.0, 10.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let on_count = rep.raw.startup.iter().filter(|s| s.enabled).count();
                ui.label(RichText::new(format!("{} de {} se abren al encender {}", on_count, rep.raw.startup.len(), diag::txt("os"))).size(12.5).color(MUTED));
                for (i, s) in rep.raw.startup.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let locked = s.machine_wide() && !admin;
                        let mut on = s.enabled;
                        if ui.add_enabled_ui(!locked, |ui| toggle(ui, &mut on)).inner.changed() {
                            toggle_item = Some((i, on));
                        }
                        let name: String = if s.name.chars().count() > 42 { s.name.chars().take(40).collect::<String>() + "…" } else { s.name.clone() };
                        ui.label(RichText::new(name).font(font(14.5)).color(if s.enabled { TEXT } else { MUTED })).on_hover_text(&s.command);
                        if s.suspicious {
                            badge(ui, "Sospechoso", DANGER);
                        } else if s.missing() {
                            badge(ui, "Roto", WARN);
                        } else if !s.signed() && !s.is_nexo() {
                            badge(ui, "Sin firma", WARN);
                        }
                        if s.machine_wide() {
                            badge(ui, "Todo el equipo", ACCENT);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let who = if !s.publisher.is_empty() {
                                s.publisher.clone()
                            } else if s.missing() {
                                "programa no encontrado".to_string()
                            } else {
                                "editor desconocido".to_string()
                            };
                            ui.label(RichText::new(who).size(12.0).color(MUTED));
                        });
                    });
                }
                if !admin && rep.raw.startup.iter().any(|s| s.machine_wide()) {
                    ui.label(RichText::new("Los de «todo el equipo» se cambian abriendo NEXO como administrador.").size(12.0).color(WARN));
                }
            });

            section(ui, "Detalles", "datos leídos de tu equipo", ACCENT);
            ui.columns(3, |c| {
                card().show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Disco").font(font(12.0)).color(MUTED));
                    for d in &rep.raw.disks {
                        ui.label(RichText::new(&d.name).font(font(14.0)));
                        let health = if d.health == "Healthy" { "buena".to_string() } else { d.health.clone() };
                        ui.label(RichText::new(format!("{} · {} · salud {}", d.media, fmt::bytes(d.size.max(0) as u64), health)).size(12.5));
                        match (d.wear, d.temp) {
                            (Some(w), Some(tc)) => ui.label(RichText::new(format!("Desgaste {} % · {} °C", w, tc)).size(12.5)),
                            _ => ui.label(RichText::new("Desgaste y temperatura: abre NEXO como administrador").size(12.0).color(MUTED)),
                        };
                    }
                    ui.label(RichText::new(format!("Libre: {} de {}", fmt::bytes(rep.disk_free), fmt::bytes(rep.disk_total))).size(12.5));
                });
                card().show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Batería").font(font(12.0)).color(MUTED));
                    let b = &rep.raw.battery;
                    match b.health() {
                        Some(h) => {
                            let col = score_color(h as u8);
                            ui.label(RichText::new(format!("{:.0} % de salud", h)).font(font(22.0)).color(col));
                            bar(ui, h / 100.0, col, 6.0);
                            ui.label(RichText::new(format!("Capacidad {:.1} Wh de {:.1} Wh originales", b.full as f32 / 1000.0, b.design as f32 / 1000.0)).size(12.5));
                            ui.label(RichText::new(format!("{} ciclos de carga", b.cycles)).size(12.5));
                        }
                        None => {
                            ui.label(RichText::new("No se detectó batería.").color(MUTED));
                        }
                    }
                    if link(ui, "Ver el informe completo del sistema", ACCENT).clicked() {
                        action = Some(Action::BatteryReport);
                    }
                });
                card().show(&mut c[2], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Sistema").font(font(12.0)).color(MUTED));
                    ui.label(RichText::new(&rep.raw.os).size(12.5));
                    ui.label(RichText::new(format!("Encendido desde hace {:.1} días", rep.raw.uptime_days)).size(12.5));
                    let upd = if rep.raw.last_update_days >= 0 { format!("Última actualización hace {} días", rep.raw.last_update_days) } else { "Última actualización: desconocida".into() };
                    ui.label(RichText::new(upd).size(12.5));
                    ui.label(RichText::new(format!("Plan de energía: {}", rep.power_name)).size(12.5));
                    if rep.power_guid != diag::BALANCED && link(ui, "Usar «Equilibrado»", ACCENT).clicked() {
                        action = Some(Action::SetBalanced);
                    }
                    if can_undo && link(ui, "Deshacer el cambio de plan", WARN).clicked() {
                        undo_power = true;
                    }
                });
            });
        });

        if rerun {
            self.start_diag();
        }
        if let Some(a) = action {
            self.do_action(a, now);
        }
        if undo_power {
            self.set_balanced(false, now);
        }
        if let Some((i, on)) = toggle_item {
            self.toggle_startup(i, on, now);
        }
    }

    /// jimy-betanzos · Pantalla Protección.
    fn page_shield(&mut self, ui: &mut Ui, t: f32) {
        header(ui, "› Escudo", "Protección", &format!("Revisa la seguridad de {} sin internet, apoyándose en {}.", diag::txt("os"), diag::txt("av")));
        let now = ui.input(|i| i.time);
        if self.diag_rx.is_some() {
            self.diag_running_view(ui, t);
            return;
        }
        let Some(rep) = self.diag.clone() else {
            self.diag_hero(ui, "Revisa la protección de tu equipo", "Antivirus, firewall, actualizaciones, programas de inicio y tareas sospechosas.", "🔒  Revisar protección");
            return;
        };
        let sec = rep.areas.iter().find(|(a, _)| a == "Seguridad").and_then(|(_, s)| *s).unwrap_or(0);
        let scanning = self.mpscan_rx.is_some();
        let mp_started = self.mp_started;
        let mp_est = if self.cfg.defender_scan_secs > 0 { self.cfg.defender_scan_secs as f64 } else { 240.0 };
        let scan_msg = self.mpscan_msg.clone();
        let vigia_running = self.vigia_running || vigia::is_running();
        let mut action: Option<Action> = None;
        let mut scan = false;
        let mut rerun = false;
        let mut cfg_changed = false;
        let cfg = &mut self.cfg;
        let r = &rep.raw;
        let d = &r.defender;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |c| {
                card().show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (rr, _) = ui.allocate_exact_size(vec2(190.0, 186.0), Sense::hover());
                        let col = score_color(sec);
                        ring(ui.painter(), rr.center(), 68.0, sec as f32 / 100.0, col, 10.0);
                        ui.painter().text(rr.center() - vec2(0.0, 6.0), Align2::CENTER_CENTER, "🛡", egui::FontId::proportional(40.0), col);
                        ui.painter().text(rr.center() + vec2(0.0, 30.0), Align2::CENTER_CENTER, format!("{}/100", sec), font(13.0), col);
                        ui.vertical(|ui| {
                            ui.add_space(26.0);
                            let (txt, col) = if sec >= 85 { ("Protegido", ACCENT) } else if sec >= 60 { ("Revisar", WARN) } else { ("En riesgo", DANGER) };
                            ui.label(RichText::new(txt).font(font(30.0)).color(col));
                            ui.label(RichText::new(format!("Revisado {}", fmt::ago(rep.ts))).size(12.5).color(MUTED));
                            ui.add_space(8.0);
                            if link(ui, "⟳ Volver a revisar", ACCENT).clicked() {
                                rerun = true;
                            }
                        });
                    });
                });
                card().show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(diag::txt("av")).font(font(13.0)).color(ACCENT));
                    if d.available {
                        let q = if d.quick_age >= 0 && d.quick_age < 10000 { format!("hace {} días", d.quick_age) } else { "nunca".into() };
                        ui.label(RichText::new(format!("Último análisis rápido: {}", q)).size(13.0));
                        ui.label(RichText::new(format!("Firmas actualizadas hace {} días · se actualizan con {}", d.sig_age, diag::txt("updater"))).size(12.5).color(MUTED));
                    } else {
                        ui.label(RichText::new(diag::txt("av_missing_title")).color(WARN));
                    }
                    ui.add_space(6.0);
                    if scanning {
                        let f = ((now - mp_started) / mp_est).min(0.95) as f32;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Analizando con {}…", diag::txt("av"))).color(ACCENT).size(13.0));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(format!("{:.0} %", f * 100.0)).font(font(15.0)).color(ACCENT));
                            });
                        });
                        progress_bar(ui, Some(f), ACCENT, t);
                        ui.label(RichText::new(format!("{} de ≈ {} (estimado; el sistema no informa el avance real). Puedes seguir usando el equipo.", fmt::secs(now - mp_started), fmt::secs(mp_est))).size(11.5).color(MUTED));
                    } else {
                        let w = ui.available_width();
                        if diag::quick_scan_available() && button(ui, "🛡  Análisis rápido", ACCENT, vec2(w, 44.0), true).clicked() {
                            scan = true;
                        }
                    }
                    if let Some((m, col)) = &scan_msg {
                        ui.label(RichText::new(m).color(*col).size(13.0));
                    }
                });
            });

            section(ui, "Comprobaciones", &format!("lectura local de la configuración de {}", diag::txt("os")), ACCENT);
            card().inner_margin(Margin::symmetric(16.0, 8.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let fw: Vec<String> = r.firewall.iter().map(|f| format!("{} {}", match f.name.as_str() { "Domain" => "Dominio", "Private" => "Privada", "Public" => "Pública", n => n }, if f.enabled { "✔" } else { "✖" })).collect();
                let susp = r.startup.iter().filter(|s| s.enabled && s.suspicious).count();
                let unsigned = r.startup.iter().filter(|s| s.enabled && !s.signed() && !s.missing() && !s.is_nexo()).count();
                let open = |k: &str| diag::setting(k).map(|t| Action::Open(t.to_string()));
                let rows: Vec<(u8, String, String, Option<Action>)> = vec![
                    (
                        if d.available && d.realtime && d.enabled { 0 } else { 2 },
                        "Antivirus en tiempo real".into(),
                        if !d.available { diag::txt("av").to_string() + " no está disponible" } else if d.realtime { format!("{} te protege mientras usas el equipo", diag::txt("av")) } else { "Desactivado: actívalo cuanto antes".into() },
                        open("realtime"),
                    ),
                    (if d.sig_age <= 7 { 0 } else { 1 }, "Firmas del antivirus".into(), format!("Actualizadas hace {} días", d.sig_age), open("updates")),
                    (if r.firewall.iter().all(|f| f.enabled) && !r.firewall.is_empty() { 0 } else { 2 }, "Firewall".into(), fw.join(" · "), open("firewall")),
                    (if r.uac == Some(false) { 2 } else if r.uac.is_none() { 3 } else { 0 }, diag::txt("uac_name").into(), match r.uac { Some(false) => "Desactivado".into(), None => "No aplica en este sistema".into(), _ => diag::txt("uac_on").into() }, None),
                    (
                        if (0..=45).contains(&r.last_update_days) { 0 } else if r.last_update_days < 0 { 3 } else { 1 },
                        format!("Actualizaciones de {}", diag::txt("os")),
                        if r.last_update_days >= 0 { format!("Última instalada hace {} días", r.last_update_days) } else { "No se pudo leer".into() },
                        open("updates"),
                    ),
                    (if r.rdp { 1 } else { 0 }, diag::txt("remote_name").into(), if r.rdp { "Activado: desactívalo si no lo usas".into() } else { "Desactivado".into() }, if r.rdp { open("remote") } else { None }),
                    (if r.proxy.is_empty() { 0 } else { 1 }, "Proxy de red".into(), if r.proxy.is_empty() { "Sin proxy: conexión directa".into() } else { format!("Configurado: {}", r.proxy) }, if r.proxy.is_empty() { None } else { open("proxy") }),
                    {
                        let unknown = diag::unknown_hosts(&r.hosts);
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
                        format!("Cifrado del disco ({})", diag::txt("encryption")),
                        match r.encryption { 1 => "Activado".into(), 2 => "Desactivado".into(), _ => "No disponible en esta edición o equipo".into() },
                        if r.encryption == 2 { open("encryption") } else { None },
                    ),
                    (if susp > 0 { 2 } else if unsigned > 0 { 1 } else { 0 }, "Programas de inicio".into(), format!("{} sospechosos · {} sin firma digital (activos)", susp, unsigned), None),
                    (if r.tasks.is_empty() { 0 } else { 2 }, "Tareas programadas".into(), format!("{} de terceros revisadas · {} sospechosas", r.tasks_total, r.tasks.len()), if r.tasks.is_empty() { None } else { open("tasks") }),
                ];
                for (state, title, detail, act) in rows {
                    if check_row(ui, state, &title, &detail, act.as_ref()) {
                        action = act;
                    }
                }
            });

            if !r.tasks.is_empty() || r.startup.iter().any(|s| s.enabled && (s.suspicious || (!s.signed() && !s.missing() && !s.is_nexo()))) {
                section(ui, "Para revisar", "si no reconoces algo, desactívalo en Diagnóstico › Programas de inicio", WARN);
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for s in r.startup.iter().filter(|s| s.enabled && (s.suspicious || (!s.signed() && !s.missing() && !s.is_nexo()))) {
                        ui.label(RichText::new(format!("{} · {}", s.name, if s.suspicious { "carpeta poco habitual" } else { "sin firma digital" })).font(font(14.0)));
                        ui.label(RichText::new(&s.exe).monospace().size(11.5).color(MUTED));
                    }
                    for tk in &r.tasks {
                        ui.label(RichText::new(format!("Tarea «{}» · {}", tk.name, tk.why)).font(font(14.0)).color(DANGER));
                        ui.label(RichText::new(&tk.exe).monospace().size(11.5).color(MUTED));
                    }
                });
            }

            section(ui, "Vigilancia continua", "el vigía revisa esto cada 30 segundos", ACCENT);
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                cfg_changed |= ui.checkbox(&mut cfg.alert_security, format!("Avisar si {} se apaga o si un programa nuevo se añade al inicio de {}", diag::txt("av"), diag::txt("os"))).changed();
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
            self.start_diag();
        }
        if scan {
            self.start_mpscan();
        }
        if let Some(a) = action {
            self.do_action(a, now);
        }
    }

    // ───────────────────────────── celulares (fase 4) ─────────────────────────────

    /// jimy-betanzos · Lanza una tarea con celulares en un hilo aparte.
    fn adb_spawn(&mut self, label: &str, f: impl FnOnce() -> AdbMsg + Send + 'static) {
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

    /// jimy-betanzos · Vuelve a buscar celulares conectados.
    fn adb_refresh(&mut self) {
        self.adb_spawn("Buscando celulares…", || AdbMsg::Devices(adb::devices()));
    }

    /// jimy-betanzos · Lee los datos de un celular.
    fn adb_load(&mut self, serial: String) {
        // Al recargar el mismo celular se conserva lo mostrado: la página no salta arriba.
        if self.dev_sel.as_deref() != Some(serial.as_str()) {
            self.dev_info = None;
        }
        self.dev_sel = Some(serial.clone());
        self.adb_spawn("Leyendo el celular…", move || {
            let info = adb::info(&serial);
            let bloat = adb::bloat(&serial).unwrap_or_default();
            let usage = adb::battery_usage(&serial).unwrap_or_default();
            AdbMsg::Loaded(serial, info, bloat, usage)
        });
    }

    /// jimy-betanzos · Recibe los resultados de las tareas con celulares.
    fn poll_adb(&mut self, now: f64) {
        let Some(msg) = self.adb_rx.as_ref().and_then(|rx| rx.try_recv().ok()) else { return };
        self.adb_rx = None;
        self.adb_busy = None;
        match msg {
            AdbMsg::Devices(Ok(d)) => {
                self.devices = d;
                let keep = self.dev_sel.as_ref().is_some_and(|s| self.devices.iter().any(|d| &d.serial == s && d.ready()));
                if !keep {
                    self.dev_info = None;
                    self.dev_sel = None;
                    if let Some(first) = self.devices.iter().find(|d| d.ready()).map(|d| d.serial.clone()) {
                        self.adb_load(first);
                    }
                }
            }
            AdbMsg::Devices(Err(e)) => self.adb_msg = Some((format!("ADB: {}", e), DANGER)),
            AdbMsg::Loaded(serial, info, bloat, usage) => {
                if self.dev_sel.as_deref() == Some(serial.as_str()) {
                    match info {
                        Ok(i) => self.dev_info = Some(i),
                        Err(e) => self.adb_msg = Some((format!("No se pudo leer el celular: {}", e), DANGER)),
                    }
                    self.dev_bloat = bloat;
                    self.dev_usage = usage;
                }
            }
            AdbMsg::Done(res, rescan) => {
                match res {
                    Ok(m) => {
                        self.show_toast(m.clone(), ACCENT, now);
                        self.adb_msg = Some((m, ACCENT));
                    }
                    Err(e) => self.adb_msg = Some((e, DANGER)),
                }
                if rescan {
                    self.adb_refresh();
                } else if let Some(s) = self.dev_sel.clone() {
                    self.adb_load(s);
                }
            }
        }
    }

    /// jimy-betanzos · Pantalla Celulares.
    fn page_mobile(&mut self, ui: &mut Ui, t: f32) {
        header(ui, "› Enlace móvil", "Celulares", "Realme C11 2021 y Honor X7 · por USB o WiFi local, sin internet.");
        if adb::adb_path().is_none() {
            tinted_card(WARN).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("No se encontró ADB (Android platform-tools).").color(WARN).font(font(15.0)));
                ui.label("Copia la carpeta «platform-tools» junto a NEXO para usar el modo potenciado.");
            });
            return;
        }
        if !self.adb_scanned {
            self.adb_scanned = true;
            self.adb_refresh();
        }
        /// jimy-betanzos · Acción pedida sobre un celular.
        enum Act {
            Refresh,
            Select(String),
            Pair,
            Connect,
            Trim,
            Install,
            Toggle(String, bool),
        }
        let mut act: Option<Act> = None;
        let busy = self.adb_busy.clone();
        let msg = self.adb_msg.clone();
        let apk = adb::mobile_apk();
        let sel = self.dev_sel.clone();
        let devices = self.devices.clone();
        let info = self.dev_info.clone();
        let (pair_addr, pair_code, conn_addr) = (&mut self.pair_addr, &mut self.pair_code, &mut self.conn_addr);
        let bloat = &self.dev_bloat;
        let usage = &self.dev_usage;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |c| {
                card().show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Tus celulares").font(font(13.0)).color(ACCENT));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if busy.is_none() && link(ui, "⟳ Buscar", ACCENT).clicked() {
                                act = Some(Act::Refresh);
                            }
                        });
                    });
                    if devices.is_empty() {
                        ui.label(RichText::new("Ningún celular conectado. Conéctalo por USB o por WiFi.").color(MUTED));
                    }
                    for d in &devices {
                        let selected = sel.as_deref() == Some(d.serial.as_str());
                        let frame = if selected { tinted_card(ACCENT) } else { card() };
                        let r = frame.inner_margin(Margin::symmetric(12.0, 8.0)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("📱").size(18.0));
                                ui.label(RichText::new(if d.model.is_empty() { d.serial.clone() } else { d.model.clone() }).font(font(15.0)));
                                badge(ui, if d.wifi() { "WiFi" } else { "USB" }, ACCENT);
                                match d.state.as_str() {
                                    "device" => badge(ui, "Listo", ACCENT),
                                    "unauthorized" => badge(ui, "Acepta el aviso en el celular", WARN),
                                    s => badge(ui, s, MUTED),
                                }
                            });
                        });
                        if d.ready() && !selected && r.response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            act = Some(Act::Select(d.serial.clone()));
                        }
                    }
                    if let Some(b) = &busy {
                        let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 70.0), Sense::hover());
                        scanner(ui.painter(), rr.center(), 24.0, t, ACCENT);
                        ui.vertical_centered(|ui| ui.label(RichText::new(b).color(ACCENT)));
                    }
                    if let Some((m, col)) = &msg {
                        ui.label(RichText::new(m).color(*col).size(13.0));
                    }
                });
                card().show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Conectar por WiFi (Android 11)").font(font(13.0)).color(ACCENT));
                    ui.label(RichText::new("En el celular: Opciones de desarrollador › Depuración inalámbrica › «Vincular con código».").size(12.5).color(MUTED));
                    ui.add(egui::TextEdit::singleline(pair_addr).hint_text(RichText::new("IP:puerto de vinculación  (ej. 192.168.1.40:37123)").color(alpha(MUTED, 160))).desired_width(f32::INFINITY));
                    ui.add(egui::TextEdit::singleline(pair_code).hint_text(RichText::new("Código de 6 dígitos").color(alpha(MUTED, 160))).desired_width(f32::INFINITY));
                    if button(ui, "Vincular", ACCENT, vec2(ui.available_width(), 36.0), busy.is_none() && !pair_addr.trim().is_empty() && !pair_code.trim().is_empty()).clicked() {
                        act = Some(Act::Pair);
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new("Después, con la IP y el puerto de «Depuración inalámbrica» (no los de vincular):").size(12.5).color(MUTED));
                    ui.add(egui::TextEdit::singleline(conn_addr).hint_text(RichText::new("IP:puerto  (ej. 192.168.1.40:41235)").color(alpha(MUTED, 160))).desired_width(f32::INFINITY));
                    if button(ui, "Conectar", ACCENT, vec2(ui.available_width(), 36.0), busy.is_none() && !conn_addr.trim().is_empty()).clicked() {
                        act = Some(Act::Connect);
                    }
                    ui.label(RichText::new("Por USB: activa «Depuración USB», conecta el cable y acepta el aviso en el celular.").size(12.0).color(MUTED));
                });
            });

            let Some(i) = &info else { return };
            section(ui, &format!("{} {} · Android {}", i.brand, i.model, i.android), "datos leídos del celular ahora mismo", ACCENT);
            ui.columns(3, |c| {
                let used = 1.0 - i.data_free as f32 / i.data_total.max(1) as f32;
                let ram = 1.0 - i.ram_avail as f32 / i.ram_total.max(1) as f32;
                let bat = i.battery as f32 / 100.0;
                for (k, (frac, col, big, label, sub)) in [
                    (used, level_color(used), format!("{:.0}%", used * 100.0), "Almacenamiento", format!("{} libres de {}", fmt::bytes(i.data_free), fmt::bytes(i.data_total))),
                    (ram, level_color(ram), format!("{:.0}%", ram * 100.0), "RAM", format!("{} libres de {}", fmt::bytes(i.ram_avail), fmt::bytes(i.ram_total))),
                    (bat, if bat < 0.2 { DANGER } else if bat < 0.4 { WARN } else { ACCENT }, format!("{}%", i.battery), "Batería", format!("{:.1} °C · salud {}", i.temp_c, i.health)),
                ]
                .into_iter()
                .enumerate()
                {
                    card().show(&mut c[k], |ui| {
                        ui.set_width(ui.available_width());
                        let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 150.0), Sense::hover());
                        ring(ui.painter(), rr.center(), 52.0, frac, col, 7.0);
                        ui.painter().text(rr.center() - vec2(0.0, 6.0), Align2::CENTER_CENTER, big, font(28.0), TEXT);
                        ui.painter().text(rr.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, label, font(10.0), col);
                        ui.vertical_centered(|ui| ui.label(RichText::new(sub).size(12.0).color(MUTED)));
                    });
                }
            });
            ui.columns(2, |c| {
                tinted_card(ACCENT).show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("⚡ Limpieza profunda").font(font(15.0)).color(ACCENT));
                    ui.label(RichText::new("Borra la caché de TODAS las apps de una vez, algo que Android 11 no deja hacer desde el propio celular.").size(12.5));
                    ui.label(RichText::new("No toca fotos, videos, mensajes, cuentas ni datos de las apps.").size(12.0).color(ACCENT));
                    if button(ui, "Borrar caché de todas las apps", ACCENT, vec2(ui.available_width(), 42.0), busy.is_none()).clicked() {
                        act = Some(Act::Trim);
                    }
                    if apk.is_some() && button(ui, "Instalar / actualizar NEXO móvil", ACCENT, vec2(ui.available_width(), 42.0), busy.is_none()).clicked() {
                        act = Some(Act::Install);
                    }
                });
                card().show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Batería por app · desde la última carga").font(font(13.0)).color(ACCENT));
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
                        bar(ui, (*mah / max) as f32, WARN, 4.0);
                    }
                });
            });
            section(ui, "Apps preinstaladas", "desactivarlas es reversible: no se borran y se pueden volver a activar", WARN);
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                if bloat.is_empty() {
                    ui.label(RichText::new("No se encontraron apps preinstaladas innecesarias de la lista conocida.").color(MUTED));
                }
                for b in bloat {
                    ui.horizontal(|ui| {
                        let mut on = b.enabled;
                        if toggle(ui, &mut on).changed() {
                            act = Some(Act::Toggle(b.pkg.clone(), on));
                        }
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&b.name).font(font(14.5)).color(if b.enabled { TEXT } else { MUTED }));
                            ui.label(RichText::new(format!("{}  ·  {}", b.why, b.pkg)).size(11.5).color(MUTED));
                        });
                    });
                }
            });
        });

        let serial = self.dev_sel.clone().unwrap_or_default();
        match act {
            Some(Act::Refresh) => self.adb_refresh(),
            Some(Act::Select(s)) => self.adb_load(s),
            Some(Act::Pair) => {
                let (a, c) = (self.pair_addr.clone(), self.pair_code.clone());
                self.pair_code.clear();
                self.adb_spawn("Vinculando…", move || AdbMsg::Done(adb::pair(&a, &c), true));
            }
            Some(Act::Connect) => {
                let a = self.conn_addr.clone();
                self.adb_spawn("Conectando…", move || AdbMsg::Done(adb::connect(&a), true));
            }
            Some(Act::Trim) => self.adb_spawn("Borrando la caché de todas las apps…", move || {
                AdbMsg::Done(adb::trim_caches(&serial).map(|f| format!("Caché borrada · {} liberados en el celular", fmt::bytes(f))), false)
            }),
            Some(Act::Install) => {
                if let Some(apk) = apk {
                    self.adb_spawn("Instalando NEXO móvil…", move || AdbMsg::Done(adb::install_apk(&serial, &apk), false));
                }
            }
            Some(Act::Toggle(pkg, on)) => self.adb_spawn(if on { "Activando…" } else { "Desactivando…" }, move || {
                AdbMsg::Done(adb::set_enabled(&serial, &pkg, on).map(|_| if on { "App activada de nuevo.".to_string() } else { "App desactivada (puedes reactivarla cuando quieras).".to_string() }), false)
            }),
            None => {}
        }
    }

    // ───────────────────────────── memoria RAM (fase 2) ─────────────────────────────

    /// jimy-betanzos · Mide los programas en otro hilo: la ventana no se traba cada 2 segundos.
    fn refresh_ram(&mut self, now: f64) {
        if self.ram_pending {
            return;
        }
        self.ram_pending = true;
        self.last_sample = now;
        let sampler = self.sampler.clone();
        self.bg(move || {
            let (groups, cpu) = sampler.lock().map(|mut s| s.sample(true)).unwrap_or_default();
            let alerts = config::load_alerts();
            let running = vigia::is_running();
            Box::new(move |a: &mut NexoApp, now| {
                a.groups = groups;
                a.cpu = cpu;
                a.snap = snapshot(now);
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

    /// jimy-betanzos · Activa o desactiva el vigía en segundo plano.
    fn set_vigia(&mut self, on: bool, now: f64) {
        if self.pending_actions > 0 {
            return;
        }
        self.show_toast(if on { "Activando el vigía…" } else { "Deteniendo el vigía…" }.into(), ACCENT, now);
        self.bg_action("Configurando el vigía", move |_progress| {
            let res: Result<(), String> = if on {
                install::install().map(|_| {
                    install::set_autostart(true);
                    if !vigia::is_running() {
                        vigia::start();
                        std::thread::sleep(Duration::from_millis(800));
                    }
                })
            } else {
                install::set_autostart(false);
                vigia::stop_and_wait();
                Ok(())
            };
            let running = vigia::is_running();
            Box::new(move |a: &mut NexoApp, now| {
                a.vigia_running = running;
                a.last_sample = -10.0;
                match res {
                    Ok(()) => {
                        a.cfg.vigia_enabled = on;
                        a.cfg.save();
                        if on {
                            a.show_toast("Vigía activado: busca el icono de NEXO junto al reloj".into(), ACCENT, now);
                        } else {
                            a.show_toast("Vigía desactivado".into(), WARN, now);
                        }
                    }
                    Err(e) => a.show_toast(format!("No se pudo activar el vigía: {}", e), DANGER, now),
                }
            })
        });
    }

    /// jimy-betanzos · Pantalla Memoria RAM.
    fn page_ram(&mut self, ui: &mut Ui) {
        header(ui, "› Estabilizador", "Memoria RAM", "Vigila la memoria y actúa solo cuando hay un problema real. Nunca cierra nada sin tu permiso.");
        let now = ui.input(|i| i.time);
        let mem_load = self.snap.mem.load;
        let frac = mem_load as f32 / 100.0;
        let (avail, total) = (self.snap.mem.avail, self.snap.mem.total);
        let commit = self.snap.mem.commit_pct();
        let cpu = self.cpu;
        let apps = self.groups.iter().filter(|g| !g.windows.is_empty()).count();
        let hist: Vec<f32> = self.ram_hist.iter().copied().collect();
        let trimmable = self.groups.iter().filter(|g| g.can_trim()).count();
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
                card().show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 196.0), Sense::hover());
                    let col = level_color(frac);
                    ring(ui.painter(), r.center(), 70.0, frac, col, 9.0);
                    ui.painter().text(r.center() - vec2(0.0, 8.0), Align2::CENTER_CENTER, format!("{}%", mem_load), font(36.0), TEXT);
                    ui.painter().text(r.center() + vec2(0.0, 22.0), Align2::CENTER_CENTER, "RAM en uso", font(11.0), col);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(format!("{} libres de {}", fmt::bytes(avail), fmt::bytes(total))).size(12.5).color(MUTED));
                    });
                });
                card().show(&mut c[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Memoria virtual").font(font(12.0)).color(MUTED));
                    ui.label(RichText::new(format!("{:.0} %", commit)).font(font(20.0)));
                    bar(ui, commit / 100.0, level_color(commit / 100.0), 6.0);
                    ui.label(RichText::new("Procesador").font(font(12.0)).color(MUTED));
                    ui.label(RichText::new(format!("{:.0} %", cpu)).font(font(20.0)));
                    bar(ui, cpu / 100.0, level_color(cpu / 100.0), 6.0);
                    ui.label(RichText::new(format!("RAM · últimos 3 min · {} programas con ventana", apps)).size(11.5).color(MUTED));
                    sparkline(ui, &hist, ACCENT);
                });
                c[2].with_layout(Layout::top_down(Align::Min), |ui| {
                    tinted_card(ACCENT).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("⚡ Estabilizar").font(font(16.0)).color(ACCENT));
                        ui.label(RichText::new(if cfg!(windows) { "Compacta los programas inactivos de segundo plano: su memoria pasa a la reserva de Windows y vuelve sola si la necesitan." } else { "Libera la caché de memoria del sistema (archivos guardados por si acaso). Te pedirá tu contraseña." }).size(12.5));
                        ui.add_space(2.0);
                        ui.label(RichText::new("No cierra nada · no pierdes datos · no toca la app que usas.").size(12.0).color(ACCENT));
                        ui.add_space(8.0);
                        let w = ui.available_width();
                        let label = if cfg!(windows) { format!("Estabilizar ahora · {}", trimmable) } else { "Estabilizar ahora".to_string() };
                        if button(ui, &label, ACCENT, vec2(w, 46.0), trimmable > 0 || cfg!(unix)).clicked() {
                            do_stabilize = true;
                        }
                    });
                });
            });

            section(ui, "Programas que más memoria usan", "agrupados por programa · se actualiza cada 2 s", ACCENT);
            card().inner_margin(Margin::symmetric(16.0, 10.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if groups.is_empty() {
                    ui.label(RichText::new("Midiendo…").color(MUTED));
                }
                for g in groups.iter().take(14) {
                    ui.horizontal(|ui| {
                        let wname = (ui.available_width() - 300.0).max(240.0);
                        ui.allocate_ui_with_layout(vec2(wname, 28.0), Layout::left_to_right(Align::Center), |ui| {
                            ui.label(RichText::new(&g.name).font(font(15.0)));
                            if g.pids.len() > 1 {
                                ui.label(RichText::new(format!("×{}", g.pids.len())).size(11.5).color(MUTED));
                            }
                            if g.system {
                                badge(ui, "Sistema", MUTED);
                            }
                            if g.foreground {
                                badge(ui, "En uso", ACCENT);
                            }
                            if g.unsaved {
                                badge(ui, "Sin guardar", ACCENT);
                            } else if g.editor {
                                badge(ui, "Trabajo", ACCENT);
                            }
                            if g.installing {
                                badge(ui, "Instalando", WARN);
                            }
                            if g.io_active() {
                                badge(ui, "Activo", WARN);
                            } else if g.can_trim() {
                                badge(ui, "Inactivo", alpha(ACCENT, 210));
                            }
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if !g.system && (!g.windows.is_empty() || g.accessible) && link(ui, "Cerrar", DANGER).on_hover_text("Antes te avisa si hay algo importante").clicked() {
                                action = Some((g.exe.clone(), true));
                            }
                            if g.can_trim() && link(ui, "Compactar", ACCENT).clicked() {
                                action = Some((g.exe.clone(), false));
                            }
                            ui.label(RichText::new(format!("{:.0} % CPU", g.cpu_pct)).size(12.0).color(MUTED));
                            let share = g.private as f32 / total.max(1) as f32;
                            ui.label(RichText::new(fmt::bytes(g.private)).font(font(16.0)).color(level_color(share * 4.0)));
                        });
                    });
                }
            });

            ui.add_space(6.0);
            ui.columns(2, |c| {
                card().show(&mut c[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let mut on = cfg.vigia_enabled;
                        if toggle(ui, &mut on).changed() {
                            vigia_toggle = Some(on);
                        }
                        ui.label(RichText::new("Vigía en segundo plano").font(font(18.0)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if vigia_running {
                                badge(ui, "En marcha", ACCENT);
                            } else {
                                badge(ui, "Detenido", MUTED);
                            }
                        });
                    });
                    ui.label(RichText::new(format!("{} que revisa cada 30 s y te avisa. Ocupa unos pocos MB y arranca con {}.", if cfg!(windows) { "Un icono junto al reloj" } else { "Un proceso en segundo plano" }, diag::txt("os"))).size(12.5).color(MUTED));
                    ui.add_space(6.0);
                    ui.label(RichText::new("Aviso de RAM alta").font(font(12.0)).color(MUTED));
                    cfg_changed |= ui.add(egui::Slider::new(&mut cfg.ram_warn, 70..=95).suffix(" %")).changed();
                    ui.label(RichText::new("RAM crítica").font(font(12.0)).color(MUTED));
                    let lo = cfg.ram_warn + 2;
                    cfg.ram_critical = cfg.ram_critical.max(lo);
                    cfg_changed |= ui.add(egui::Slider::new(&mut cfg.ram_critical, lo..=98).suffix(" %")).changed();
                    cfg_changed |= ui.checkbox(&mut cfg.auto_stabilize, "Estabilizar solo si la RAM llega a crítica (nunca cierra nada)").changed();
                    cfg_changed |= ui.checkbox(&mut cfg.alert_leaks, "Avisar de fugas de memoria").changed();
                    cfg_changed |= ui.checkbox(&mut cfg.alert_cpu, "Avisar si el procesador pasa 3 min al máximo").changed();
                    cfg_changed |= ui.checkbox(&mut cfg.alert_disk, "Avisar si el disco baja del 10 % libre").changed();
                    ui.label(RichText::new("En pantalla completa (juegos, videos) solo llegan los avisos críticos.").size(12.0).color(MUTED));
                    if link(ui, "Probar una notificación", ACCENT).clicked() {
                        test_toast = true;
                    }
                });
                c[1].with_layout(Layout::top_down(Align::Min), |ui| {
                    card().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new("Últimos avisos del vigía").font(font(13.0)).color(ACCENT));
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
                                badge(ui, &a.kind, col);
                                ui.label(RichText::new(fmt::ago(a.ts)).size(11.5).color(MUTED));
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
            self.show_toast("Estabilizando…".into(), ACCENT, now);
            self.bg_action("Estabilizando la memoria", move |progress| {
                let (n, freed) = procs::trim_idle_progress(&groups, &progress);
                let text = if cfg!(windows) {
                    format!("Compactados {} programas inactivos · {} recuperados. No se cerró nada.", n, fmt::bytes(freed))
                } else if n > 0 {
                    format!("Memoria del sistema liberada · {} recuperados. No se cerró nada.", fmt::bytes(freed))
                } else {
                    "No se liberó memoria: se canceló el permiso.".to_string()
                };
                config::push_alert("estabilizada", &text);
                Box::new(move |a: &mut NexoApp, now| {
                    a.show_toast(text, ACCENT, now);
                    a.last_sample = -10.0;
                })
            });
        }
        if let Some((exe, close)) = action {
            if let Some(g) = self.groups.iter().find(|g| g.exe == exe).cloned() {
                if close {
                    self.close = Some(CloseState { g, sent_at: None, ack: false, force_ack: false, done: None });
                } else {
                    self.bg_action("Compactando el programa", move |_progress| {
                        let f = procs::trim(&g);
                        Box::new(move |a: &mut NexoApp, now| {
                            a.show_toast(format!("{} compactado · {} recuperados", g.name, fmt::bytes(f)), ACCENT, now);
                            a.last_sample = -10.0;
                        })
                    });
                }
            }
        }
        if let Some(on) = vigia_toggle {
            self.set_vigia(on, now);
        }
        if cfg_changed {
            self.cfg_dirty = Some(now);
        }
        if test_toast {
            notify::toast("Prueba de aviso", "Así se verán los avisos de NEXO.", "Es solo una prueba: no pasa nada.");
        }
    }

    /// jimy-betanzos · Ventana para cerrar un programa con cuidado.
    fn close_modal(&mut self, ctx: &egui::Context, now: f64) {
        let exe = self.close.as_ref().map(|c| c.g.exe.clone()).unwrap_or_default();
        let alive = sys::running_processes().contains(&exe);
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
        let reasons = g.reasons();
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
                card().fill(PANEL).stroke(Stroke::new(1.5_f32, alpha(ACCENT, 150))).inner_margin(Margin::same(24.0)).show(ui, |ui| {
                    ui.set_width(580.0);
                    ui.label(RichText::new("› Cerrar programa").font(font(12.0)).color(ACCENT));
                    ui.label(RichText::new(format!("Cerrar {}", g.name)).font(font(24.0)));
                    ui.label(RichText::new(format!("{} procesos · {} en memoria", g.pids.len(), fmt::bytes(g.private))).color(MUTED));
                    ui.add_space(6.0);

                    if let Some(msg) = &st.done {
                        tinted_card(ACCENT).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(msg).color(ACCENT).font(font(15.0)));
                        });
                        ui.add_space(8.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if button(ui, "Listo", ACCENT, vec2(140.0, 44.0), true).clicked() {
                                cancel = true;
                            }
                        });
                        return;
                    }
                    if g.system {
                        ui.label(RichText::new(format!("Es parte de {}: NEXO no lo cierra.", diag::txt("os"))).color(WARN));
                    }
                    if important {
                        tinted_card(ACCENT).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new("⚠  Antes de cerrar").font(font(14.0)).color(ACCENT));
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
                        tinted_card(DANGER).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new("Forzar el cierre pierde todo lo que no esté guardado.").color(DANGER));
                            ui.checkbox(&mut st.force_ack, "Sí, forzar el cierre aunque pierda cambios");
                        });
                    }
                    ui.add_space(8.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if show_force && button(ui, "Forzar cierre", DANGER, vec2(180.0, 44.0), st.force_ack && (!important || st.ack || st.sent_at.is_some())).clicked() {
                            force = true;
                        }
                        if !g.system && st.sent_at.is_none() && has_win && button(ui, "Cerrar normalmente", ACCENT, vec2(240.0, 44.0), !important || st.ack).clicked() {
                            send = true;
                        }
                        if button(ui, "Cancelar", MUTED, vec2(140.0, 44.0), true).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        ctx.move_to_top(area.response.layer_id);
        if send {
            procs::close_gracefully(&g);
            st.sent_at = Some(now);
        }
        if force {
            let n = procs::force_kill(&g);
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

    /// jimy-betanzos · Dibuja el aviso breve.
    fn toast_ui(&mut self, ctx: &egui::Context, now: f64) {
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
                tinted_card(col).fill(alpha(PANEL, 245)).show(ui, |ui| {
                    ui.label(RichText::new(text).font(font(14.0)).color(col));
                });
            });
        ctx.request_repaint_after(Duration::from_secs_f64((until - now).max(0.05)));
    }
}

/// jimy-betanzos · Color según la nota (normal, aviso o grave).
fn score_color(s: u8) -> Color32 {
    match s {
        85..=100 => ACCENT,
        70..=84 => ACCENT,
        55..=69 => WARN,
        _ => DANGER,
    }
}

/// jimy-betanzos · Fila de comprobación: 0 bien · 1 revisar · 2 importante · 3 informativo.
/// Devuelve si se pulsó su botón de acción.
fn check_row(ui: &mut Ui, state: u8, title: &str, detail: &str, action: Option<&Action>) -> bool {
    let (icon, col) = match state {
        0 => ("✔", ACCENT),
        1 => ("⚠", WARN),
        2 => ("⚠", DANGER),
        _ => ("•", ACCENT),
    };
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon).color(col).size(16.0));
        ui.label(RichText::new(title).font(font(14.5)));
        ui.label(RichText::new(detail).size(12.5).color(if state == 0 { MUTED } else { col }));
        if let Some(a) = action {
            if state != 0 {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    clicked = link(ui, a.label(), ACCENT).clicked();
                });
            }
        }
    });
    clicked
}

/// jimy-betanzos · Botón de la barra de título (minimizar, maximizar, cerrar).
fn title_btn(ui: &mut Ui, text: &str, hover: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(42.0, 30.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 6.0, alpha(hover, 50));
    }
    p.text(rect.center(), Align2::CENTER_CENTER, text, egui::FontId::proportional(15.0), if resp.hovered() { hover } else { MUTED });
    resp
}

/// jimy-betanzos · Tarjeta de una categoría con su casilla y tamaño.
fn category_card(ui: &mut Ui, r: &mut CatResult, admin: bool, thumbs: &mut ThumbCache) {
    let color = match r.kind {
        Kind::Safe => ACCENT,
        Kind::Review => WARN,
        Kind::Protected => ACCENT,
    };
    let locked = r.admin_only && !admin;
    let has = r.size > 0 || (r.special == Special::RecycleBin && r.count > 0);
    let frame = if r.selected_size() > 0 { card().stroke(Stroke::new(1.0_f32, alpha(color, 120))) } else { card() };
    frame.inner_margin(Margin::symmetric(16.0, 12.0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            if r.whole() {
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
                    ui.label(RichText::new(r.name).font(font(16.0)));
                    match r.kind {
                        Kind::Safe => badge(ui, "Seguro", ACCENT),
                        Kind::Review => badge(ui, "Revisar", WARN),
                        Kind::Protected => badge(ui, "Protegido", ACCENT),
                    }
                    if locked {
                        badge(ui, "Admin", ACCENT);
                    }
                });
                ui.label(RichText::new(r.desc).size(12.5).color(MUTED));
                for n in &r.notes {
                    ui.label(RichText::new(format!("⚠ {}", n)).size(12.0).color(WARN));
                }
            });
            ui.with_layout(Layout::top_down(Align::Max), |ui| {
                if has {
                    ui.label(RichText::new(fmt::bytes(r.size)).font(font(20.0)).color(color));
                    ui.label(RichText::new(format!("{} elementos", fmt::count(r.count))).size(12.0).color(MUTED));
                } else if !locked {
                    ui.label(RichText::new("✔ Limpio").font(font(14.0)).color(alpha(ACCENT, 190)));
                }
            });
        });

        if !r.items.is_empty() {
            let whole = r.whole();
            ui.horizontal(|ui| {
                let lbl = if r.expanded {
                    "‹  Ocultar archivos"
                } else if whole {
                    "Ver muestra de archivos  ›"
                } else {
                    "Ver y elegir archivos  ›"
                };
                if link(ui, lbl, ACCENT).clicked() {
                    r.expanded = !r.expanded;
                }
                if r.expanded && !whole {
                    ui.add_space(12.0);
                    if link(ui, "Seleccionar todo", MUTED).clicked() {
                        r.items.iter_mut().for_each(|i| i.selected = true);
                    }
                    if link(ui, "Ninguno", MUTED).clicked() {
                        r.items.iter_mut().for_each(|i| i.selected = false);
                    }
                }
            });
            if r.expanded {
                items_list(ui, r, thumbs);
            }
        }
    });
    ui.add_space(4.0);
}

/// jimy-betanzos · Cuadro de miniatura: la imagen real si existe, o el tipo de archivo mientras carga.
fn draw_thumb(ui: &Ui, rect: egui::Rect, tex: Option<Option<egui::TextureHandle>>, path: &Path) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, alpha(PANEL_HI, 230));
    match tex {
        Some(Some(t)) => {
            let s = t.size_vec2();
            let k = (rect.width() / s.x).min(rect.height() / s.y);
            let r = egui::Rect::from_center_size(rect.center(), s * k);
            p.image(t.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        other => {
            let ext = rules::ext_of(path).to_uppercase();
            let col = if other.is_none() { alpha(ACCENT, 120) } else { MUTED };
            p.text(rect.center(), Align2::CENTER_CENTER, if ext.is_empty() { "?".into() } else { ext }, font(10.5), col);
        }
    }
    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, alpha(ACCENT, 70)));
}

/// jimy-betanzos · Lista de archivos de muestra de una categoría.
fn items_list(ui: &mut Ui, r: &mut CatResult, thumbs: &mut ThumbCache) {
    let whole = r.whole();
    let protected = r.kind == Kind::Protected;
    let total = r.count;
    let id = r.id;
    let n = r.items.len();
    ui.label(RichText::new("Señala un archivo para verlo en el visor de la derecha · «Ver» lo abre con su programa.").size(11.5).color(MUTED));
    let mut hovered: Option<usize> = None;
    let height = 330.0;
    ui.horizontal_top(|ui| {
        let list_w = (ui.available_width() * 0.6).max(360.0);
        // ── Lista con miniaturas (solo se dibujan y cargan las filas visibles)
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
                            draw_thumb(ui, tr, thumbs.get(&it.path, SMALL), &it.path);
                            let w = (ui.available_width() - 120.0).max(120.0);
                            ui.allocate_ui_with_layout(vec2(w, 46.0), Layout::top_down(Align::LEFT), |ui| {
                                let name = it.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                                let (icon, col) = match rules::protection(&it.path) {
                                    Protect::Video => ("🎬 ", ACCENT),
                                    Protect::Document => ("📄 ", ACCENT),
                                    Protect::No => ("", TEXT),
                                };
                                ui.add(egui::Label::new(RichText::new(format!("{}{}", icon, name)).size(13.5).color(col)).truncate());
                                let folder = it.path.parent().map(|d| fmt::short_path(d, 60)).unwrap_or_default();
                                ui.add(egui::Label::new(RichText::new(folder).monospace().size(10.5).color(MUTED)).truncate());
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if link(ui, "Ver", ACCENT).on_hover_text("Abrir con su programa").clicked() {
                                    sys::open_file(&it.path);
                                }
                                ui.label(RichText::new(fmt::bytes(it.size)).size(12.0));
                            });
                        });
                        if ui.rect_contains_pointer(row.response.rect) {
                            hovered = Some(i);
                            ui.painter().rect_stroke(row.response.rect.expand(1.0), 0.0, Stroke::new(1.0_f32, alpha(ACCENT, 160)));
                        }
                    }
                });
        });
        if hovered.is_some() {
            r.preview = hovered;
        }
        // ── Visor
        let shown = r.preview.filter(|i| *i < n).unwrap_or(0);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::top_down(Align::Center), |ui| {
            tinted_card(ACCENT).inner_margin(Margin::same(12.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(height - 26.0);
                ui.label(RichText::new("Visor").font(font(11.0)).color(ACCENT));
                let Some(it) = r.items.get_mut(shown) else { return };
                let box_w = ui.available_width();
                let box_h = 190.0;
                let (bx, _) = ui.allocate_exact_size(vec2(box_w, box_h), Sense::hover());
                let p = ui.painter();
                p.rect_filled(bx, 0.0, alpha(BG, 200));
                match thumbs.get(&it.path, BIG) {
                    Some(Some(t)) => {
                        let s = t.size_vec2();
                        let k = (bx.width() / s.x).min(bx.height() / s.y).min(1.5);
                        let rr = egui::Rect::from_center_size(bx.center(), s * k);
                        p.image(t.id(), rr, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    Some(None) => {
                        p.text(bx.center(), Align2::CENTER_CENTER, "Sin vista previa", font(13.0), MUTED);
                    }
                    None => {
                        p.text(bx.center(), Align2::CENTER_CENTER, "Cargando…", font(13.0), ACCENT);
                    }
                }
                p.rect_stroke(bx, 0.0, Stroke::new(1.0_f32, alpha(ACCENT, 80)));
                let name = it.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                ui.add(egui::Label::new(RichText::new(name).font(font(14.0))).truncate());
                let when = std::fs::metadata(&it.path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| fmt::date(d.as_secs()))
                    .unwrap_or_default();
                ui.label(RichText::new(format!("{} · {}", fmt::bytes(it.size), when)).size(11.5).color(MUTED));
                if protected {
                    ui.label(RichText::new(format!("Origen: {}", it.origin)).size(11.5).color(ACCENT));
                }
                ui.horizontal(|ui| {
                    if link(ui, "Abrir", ACCENT).clicked() {
                        sys::open_file(&it.path);
                    }
                    if link(ui, "Mostrar en carpeta", ACCENT).clicked() {
                        sys::reveal_in_explorer(&it.path);
                    }
                    if !whole {
                        let (txt, col) = if it.selected { ("Marcado: se borrará", DANGER) } else { ("Se conserva", ACCENT) };
                        if link(ui, txt, col).on_hover_text("Cambiar").clicked() {
                            it.selected = !it.selected;
                        }
                    }
                });
            });
        });
    });
    if whole && total > n as u64 {
        ui.label(RichText::new(format!("… y {} archivos más (se muestra una muestra)", fmt::count(total - n as u64))).size(12.0).color(MUTED));
    }
}

/// jimy-betanzos · Permite cambiar el tamaño de la ventana desde los bordes.
fn resize_edges(ctx: &egui::Context) {
    // jimy-betanzos · dependencia: interfaz gráfica (egui)
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

/// jimy-betanzos · Implementación de eframe::App para NexoApp.
impl eframe::App for NexoApp {
    /// jimy-betanzos · Dibuja la ventana en cada cuadro.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_inner(ctx);
    }
}

/// jimy-betanzos · Métodos de NexoApp.
impl NexoApp {
    /// jimy-betanzos · Modo de documentación (no lo usa el usuario): con `NEXO_CAPTURA=archivo.bmp`, NEXO
    /// se fotografía a sí mismo pasados `NEXO_CAPTURA_SEG` segundos y se cierra. Con
    /// `NEXO_CAPTURA_ESCANEAR=1` antes lanza un escaneo (solo lectura) para mostrar resultados.
    fn capture_step(&mut self, ctx: &egui::Context, now: f64) {
        // jimy-betanzos · dependencia: biblioteca estándar de Rust (sync)
        use std::sync::atomic::{AtomicU8, Ordering};
        static STATE: AtomicU8 = AtomicU8::new(0); // 0 inicio · 1 esperando · 2 pedida · 3 lista
        let Some(out) = std::env::var_os("NEXO_CAPTURA") else { return };
        let delay: f64 = std::env::var("NEXO_CAPTURA_SEG").ok().and_then(|s| s.parse().ok()).unwrap_or(4.0);
        match STATE.load(Ordering::Relaxed) {
            0 => {
                if std::env::var_os("NEXO_CAPTURA_ESCANEAR").is_some() {
                    self.spawn_scan(None, false);
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
                    let _ = std::fs::write(&out, crate::thumbs::bmp(img.size[0], img.size[1], img.as_raw()));
                    STATE.store(3, Ordering::Relaxed);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            _ => {}
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    /// jimy-betanzos · Contenido principal de cada cuadro.
    fn update_inner(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let t = now as f32;
        self.poll(now);
        if now - self.snap.at > 2.5 {
            self.snap = snapshot(now);
        }

        self.now = now;
        self.poll_jobs(now);
        if self.thumbs.poll(ctx) {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
        self.poll_phase3(now);
        self.poll_adb(now);
        self.capture_step(ctx, now);

        // La lista de programas solo se mide mientras la pantalla de RAM está abierta.
        if self.page == Page::Ram && now - self.last_sample > 2.0 {
            self.refresh_ram(now);
        }

        let screen = ctx.screen_rect();
        background(&ctx.layer_painter(egui::LayerId::background()), screen);
        self.titlebar(ctx);
        self.sidebar(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().inner_margin(Margin { left: 28.0, right: 28.0, top: 14.0, bottom: 18.0 }))
            .show(ctx, |ui| match self.page {
                Page::Panel => self.page_panel(ui),
                Page::Clean => self.page_clean(ui, t),
                Page::Auto => self.page_auto(ui, t),
                Page::History => self.page_history(ui),
                Page::Ram => self.page_ram(ui),
                Page::Diag => self.page_diag(ui, t),
                Page::Shield => self.page_shield(ui, t),
                Page::Mobile => self.page_mobile(ui, t),
            });

        if self.confirm {
            self.confirm_modal(ctx);
        }
        if self.close.is_some() {
            self.close_modal(ctx, now);
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        self.ops_view(ctx, now);
        self.toast_ui(ctx, now);
        let frame_p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("border")));
        frame_p.rect_stroke(screen.shrink(0.5), 0.0, Stroke::new(1.0_f32, PANEL_HI));
        resize_edges(ctx);

        // Tras arrancar, terminar un trabajo o minimizar: soltar la memoria que ya no se usa.
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        if minimized && !self.was_minimized {
            self.trim_in = 1;
        }
        self.was_minimized = minimized;
        if self.busy.is_none() && self.trim_in > 0 {
            self.trim_in -= 1;
            if self.trim_in == 0 {
                sys::trim_own_memory();
            } else {
                ctx.request_repaint_after(Duration::from_millis(400));
            }
        }

        // Repintar solo cuando hace falta: así la CPU queda en ~0 % en reposo.
        if let Some(t) = self.cfg_dirty {
            if now - t > 0.5 && !ctx.input(|i| i.pointer.any_down()) {
                self.cfg.save();
                self.cfg_dirty = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(300));
            }
        }
        if !self.jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.busy.is_some() || self.diag_rx.is_some() || (self.adb_rx.is_some() && self.page == Page::Mobile) || (self.mpscan_rx.is_some() && self.page == Page::Shield) {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if self.mpscan_rx.is_some() {
            ctx.request_repaint_after(Duration::from_secs(1));
        } else if self.page == Page::Ram {
            ctx.request_repaint_after(Duration::from_secs(2));
        } else if self.page == Page::Panel {
            ctx.request_repaint_after(Duration::from_secs(3));
        }
    }
}
