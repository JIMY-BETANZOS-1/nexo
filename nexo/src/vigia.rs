//! Vigía: proceso diminuto en la bandeja del sistema (junto al reloj).
//!
//! Cada 30 s revisa RAM, memoria virtual, CPU, fugas de memoria y disco. Avisa con
//! notificaciones discretas y, si la RAM llega a nivel crítico, compacta SOLO programas
//! inactivos de segundo plano. Nunca cierra nada por su cuenta.
//! Sin interfaz gráfica pesada: solo la API de Windows, para ocupar unos pocos MB.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant, SystemTime};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIIF_WARNING, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow, DispatchMessageW, FindWindowW, GetCursorPos, GetMessageW, KillTimer, PostMessageW,
    PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SetForegroundWindow, SetTimer,
    TrackPopupMenu, TranslateMessage, HICON, ICONINFO, MF_SEPARATOR, MF_STRING, MSG, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY, WM_LBUTTONDBLCLK,
    WM_NULL, WM_RBUTTONUP, WM_TIMER, WM_USER, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_POPUP,
};

use crate::config::{self, Config};
use crate::procs::{self, AppGroup, Sampler};
use crate::{fmt, icon, install, sys};

const CLASS: &str = "NEXO_VIGIA";
const WM_TRAY: u32 = WM_APP + 1;
const NIN_BALLOONUSERCLICK: u32 = WM_USER + 5;
const TICK_MS: u32 = 30_000;
const LEAK_SAMPLES: usize = 20; // 20 × 30 s = 10 min
const LEAK_BYTES: u64 = 1 << 30;

const CMD_OPEN: usize = 1;
const CMD_STABILIZE: usize = 2;
const CMD_PAUSE: usize = 3;
const CMD_EXIT: usize = 4;

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn copy_w(dst: &mut [u16], s: &str) {
    let v: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..v.len()].copy_from_slice(&v);
    dst[v.len()] = 0;
}

pub fn find() -> HWND {
    let cls = w(CLASS);
    unsafe { FindWindowW(cls.as_ptr(), null()) }
}

pub fn is_running() -> bool {
    !find().is_null()
}

pub fn start() -> bool {
    std::process::Command::new(install::installed_exe()).arg("--vigia").spawn().is_ok()
}

/// Pide al vigía que se cierre y espera a que suelte el ejecutable. Devuelve si estaba activo.
pub fn stop_and_wait() -> bool {
    let h = find();
    if h.is_null() {
        return false;
    }
    unsafe {
        PostMessageW(h, WM_CLOSE, 0, 0);
    }
    for _ in 0..40 {
        if find().is_null() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(400));
    true
}

struct State {
    hwnd: HWND,
    icon: HICON,
    sampler: Sampler,
    cfg: Config,
    cfg_mtime: Option<SystemTime>,
    last_alert: HashMap<String, Instant>,
    last_stabilize: Option<Instant>,
    high: u32,
    crit: u32,
    cpu_hi: u32,
    hist: HashMap<String, VecDeque<u64>>,
    paused_until: Option<Instant>,
    taskbar_msg: u32,
    defender_seen: bool,
    defender_missing: u32,
    startup: Option<HashSet<String>>,
}

thread_local! {
    static ST: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Acceso al estado sin reentradas (si ya está en uso, se omite este mensaje).
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    ST.with(|s| s.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

fn make_icon(n: u32) -> HICON {
    unsafe {
        let mut px = icon::rgba(n);
        for p in px.chunks_mut(4) {
            p.swap(0, 2);
        }
        let color = CreateBitmap(n as i32, n as i32, 1, 32, px.as_ptr() as *const c_void);
        let mask_bits = vec![0u8; (n as usize).div_ceil(16) * 2 * n as usize];
        let mask = CreateBitmap(n as i32, n as i32, 1, 1, mask_bits.as_ptr() as *const c_void);
        let ii = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let h = CreateIconIndirect(&ii);
        DeleteObject(color);
        DeleteObject(mask);
        h
    }
}

fn nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut n: NOTIFYICONDATAW = unsafe { zeroed() };
    n.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = hwnd;
    n.uID = 1;
    n
}

fn tray_add(hwnd: HWND, icon: HICON) {
    let mut n = nid(hwnd);
    n.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    n.uCallbackMessage = WM_TRAY;
    n.hIcon = icon;
    copy_w(&mut n.szTip, "NEXO · Vigía activo");
    unsafe {
        Shell_NotifyIconW(NIM_ADD, &n);
    }
}

fn tray_tip(hwnd: HWND, text: &str) {
    let mut n = nid(hwnd);
    n.uFlags = NIF_TIP;
    copy_w(&mut n.szTip, text);
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &n);
    }
}

fn balloon(hwnd: HWND, title: &str, text: &str, warn: bool) {
    let mut n = nid(hwnd);
    n.uFlags = NIF_INFO;
    copy_w(&mut n.szInfoTitle, title);
    copy_w(&mut n.szInfo, text);
    n.dwInfoFlags = if warn { NIIF_WARNING } else { NIIF_INFO };
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &n);
    }
}

fn open_nexo(page: Option<&str>) {
    let exe = std::env::current_exe().unwrap_or_else(|_| install::installed_exe());
    let mut c = std::process::Command::new(exe);
    if let Some(p) = page {
        c.args(["--pagina", p]);
    }
    let _ = c.spawn();
}

fn top_consumers(groups: &[AppGroup], n: usize) -> String {
    groups
        .iter()
        .filter(|g| !g.system)
        .take(n)
        .map(|g| format!("{} {}", g.name, fmt::bytes(g.private)))
        .collect::<Vec<_>>()
        .join(" · ")
}

impl State {
    fn paused(&self) -> bool {
        self.paused_until.is_some_and(|t| t > Instant::now())
    }

    fn alert(&mut self, key: &str, cooldown: Duration, title: &str, text: &str, warn: bool, suppressed: bool) {
        if suppressed {
            return;
        }
        if self.last_alert.get(key).is_some_and(|t| t.elapsed() < cooldown) {
            return;
        }
        self.last_alert.insert(key.to_string(), Instant::now());
        balloon(self.hwnd, title, text, warn);
        config::push_alert(key.split(':').next().unwrap_or(key), &format!("{}: {}", title, text));
    }

    fn stabilize(&mut self, groups: &[AppGroup]) -> (usize, u64) {
        self.last_stabilize = Some(Instant::now());
        procs::trim_idle(groups)
    }

    fn tick(&mut self) {
        let mtime = config::config_mtime();
        if mtime != self.cfg_mtime {
            self.cfg = Config::load();
            self.cfg_mtime = mtime;
        }
        let mem = sys::memory();
        let (groups, cpu) = self.sampler.sample(false);
        tray_tip(self.hwnd, &format!("NEXO Vigía · RAM {} % · CPU {:.0} %", mem.load, cpu));

        let paused = self.paused();
        let busy = sys::user_busy();

        // Fugas: un programa que crece sin parar durante 10 minutos.
        if self.cfg.alert_leaks {
            let mut seen = HashSet::new();
            let mut leak: Option<(String, u64)> = None;
            for g in groups.iter().filter(|g| !g.system).take(40) {
                seen.insert(g.exe.clone());
                let h = self.hist.entry(g.exe.clone()).or_default();
                h.push_back(g.private);
                if h.len() > LEAK_SAMPLES {
                    h.pop_front();
                }
                if h.len() == LEAK_SAMPLES {
                    let (first, last) = (h[0], h[LEAK_SAMPLES - 1]);
                    let drops = h.iter().zip(h.iter().skip(1)).filter(|(a, b)| b < a).count();
                    if last > first + LEAK_BYTES && drops <= 3 {
                        leak = Some((g.name.clone(), last - first));
                    }
                }
            }
            self.hist.retain(|k, _| seen.contains(k));
            if let Some((name, grow)) = leak {
                self.alert(
                    &format!("fuga:{}", name),
                    Duration::from_secs(3600),
                    "Posible fuga de memoria",
                    &format!("{} creció {} en 10 min sin parar. Si va lento, guarda tu trabajo y reinícialo.", name, fmt::bytes(grow)),
                    true,
                    paused || busy,
                );
            }
        }

        // RAM: dos lecturas seguidas (1 min) por encima del umbral para no avisar por picos.
        let load = mem.load.min(100) as u8;
        self.crit = if load >= self.cfg.ram_critical { self.crit + 1 } else { 0 };
        self.high = if load >= self.cfg.ram_warn { self.high + 1 } else { 0 };
        if self.crit >= 2 {
            let recent = self.last_stabilize.is_some_and(|t| t.elapsed() < Duration::from_secs(600));
            if self.cfg.auto_stabilize && !recent {
                let (n, freed) = self.stabilize(&groups);
                self.alert(
                    "critica",
                    Duration::from_secs(900),
                    "Memoria crítica · estabilizada",
                    &format!(
                        "RAM al {} %. Compacté {} programas inactivos y recuperé {}. No se cerró nada. Más consumo: {}",
                        load, n, fmt::bytes(freed), top_consumers(&groups, 3)
                    ),
                    true,
                    paused,
                );
            } else {
                self.alert(
                    "critica",
                    Duration::from_secs(900),
                    "Memoria crítica",
                    &format!("RAM al {} %. Más consumo: {}. Abre NEXO para estabilizar o cerrar algo.", load, top_consumers(&groups, 3)),
                    true,
                    paused,
                );
            }
        } else if self.high >= 2 {
            self.alert(
                "ram",
                Duration::from_secs(1200),
                "Memoria alta",
                &format!("RAM al {} %. Más consumo: {}.", load, top_consumers(&groups, 3)),
                false,
                paused || busy,
            );
        }

        // Memoria virtual: si se agota, los programas empiezan a fallar.
        if mem.commit_pct() >= 90.0 {
            self.alert(
                "virtual",
                Duration::from_secs(900),
                "Memoria virtual casi agotada",
                &format!("Al {:.0} %. Guarda tu trabajo: los programas podrían fallar. Más consumo: {}", mem.commit_pct(), top_consumers(&groups, 2)),
                true,
                paused,
            );
        }

        // CPU al máximo durante 3 minutos.
        self.cpu_hi = if cpu >= 90.0 { self.cpu_hi + 1 } else { 0 };
        if self.cfg.alert_cpu && self.cpu_hi >= 6 {
            if let Some(g) = groups
                .iter()
                .filter(|g| g.exe != "system idle process")
                .max_by(|a, b| a.cpu_pct.total_cmp(&b.cpu_pct))
            {
                self.alert(
                    "cpu",
                    Duration::from_secs(1800),
                    "Procesador al máximo",
                    &format!("Lleva 3 minutos por encima del 90 %. Principal responsable: {} ({:.0} %).", g.name, g.cpu_pct),
                    false,
                    paused || busy,
                );
            }
        }

        // Disco del sistema por debajo del 10 % libre.
        if self.cfg.alert_disk {
            let (free, total) = sys::disk_space(&sys::system_drive());
            if total > 0 && (free as f64 / total as f64) < 0.10 {
                self.alert(
                    "disco",
                    Duration::from_secs(12 * 3600),
                    "Disco casi lleno",
                    &format!("Quedan {} libres. Abre NEXO › Limpieza para recuperar espacio.", fmt::bytes(free)),
                    true,
                    paused || busy,
                );
            }
        }

        // Protección: el antivirus se apaga o algo nuevo se instala para arrancar solo.
        if self.cfg.alert_security {
            if groups.iter().any(|g| g.exe == "msmpeng.exe") {
                self.defender_seen = true;
                self.defender_missing = 0;
            } else if self.defender_seen {
                self.defender_missing += 1;
                if self.defender_missing >= 2 {
                    self.alert(
                        "defensor",
                        Duration::from_secs(3600),
                        "Microsoft Defender no está funcionando",
                        "El antivirus de Windows se detuvo. Si no lo hiciste tú, abre Seguridad de Windows y actívalo.",
                        true,
                        paused,
                    );
                }
            }
            let now_entries = sys::startup_entries();
            if let Some(prev) = &self.startup {
                let new: Vec<String> = now_entries.difference(prev).cloned().collect();
                if let Some(first) = new.first() {
                    let more = if new.len() > 1 { format!(" (y {} más)", new.len() - 1) } else { String::new() };
                    self.alert(
                        &format!("inicio:{}", first),
                        Duration::from_secs(24 * 3600),
                        "Nuevo programa al inicio de Windows",
                        &format!("{}{}. Si no lo instalaste tú, revísalo en NEXO › Protección.", first, more),
                        true,
                        paused,
                    );
                }
            }
            self.startup = Some(now_entries);
        }

        sys::trim_own_memory();
    }
}

unsafe fn show_menu(hwnd: HWND, paused: bool) -> usize {
    let m = CreatePopupMenu();
    let items: [(usize, &str); 3] = [
        (CMD_OPEN, "Abrir NEXO"),
        (CMD_STABILIZE, "Estabilizar memoria ahora"),
        (CMD_PAUSE, if paused { "Reanudar avisos" } else { "Pausar avisos 1 hora" }),
    ];
    for (id, text) in items {
        let t = w(text);
        AppendMenuW(m, MF_STRING, id, t.as_ptr());
    }
    AppendMenuW(m, MF_SEPARATOR, 0, null());
    let t = w("Cerrar el vigía");
    AppendMenuW(m, MF_STRING, CMD_EXIT, t.as_ptr());
    let mut pt: POINT = zeroed();
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null());
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(m);
    cmd.max(0) as usize
}

fn command(hwnd: HWND, cmd: usize) {
    match cmd {
        CMD_OPEN => open_nexo(None),
        CMD_STABILIZE => {
            with_state(|s| {
                let (groups, _) = s.sampler.sample(false);
                let (n, freed) = s.stabilize(&groups);
                let text = format!("Compacté {} programas inactivos y recuperé {}. No se cerró nada.", n, fmt::bytes(freed));
                balloon(s.hwnd, "Memoria estabilizada", &text, false);
                config::push_alert("estabilizada", &text);
            });
        }
        CMD_PAUSE => {
            with_state(|s| {
                s.paused_until = if s.paused() { None } else { Some(Instant::now() + Duration::from_secs(3600)) };
            });
        }
        CMD_EXIT => unsafe {
            DestroyWindow(hwnd);
        },
        _ => {}
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            if wp == 2 {
                KillTimer(hwnd, 2);
            }
            with_state(|s| s.tick());
            0
        }
        WM_TRAY => {
            match (lp & 0xFFFF) as u32 {
                WM_RBUTTONUP | WM_CONTEXTMENU => {
                    let paused = with_state(|s| s.paused()).unwrap_or(false);
                    let cmd = show_menu(hwnd, paused);
                    command(hwnd, cmd);
                }
                WM_LBUTTONDBLCLK => open_nexo(None),
                NIN_BALLOONUSERCLICK => open_nexo(Some("ram")),
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => {
            // Si el Explorador se reinicia, el icono de la bandeja se vuelve a poner.
            if let Some((tb, icon)) = with_state(|s| (s.taskbar_msg, s.icon)) {
                if tb != 0 && msg == tb {
                    tray_add(hwnd, icon);
                    return 0;
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
    }
}

pub fn run() {
    if is_running() {
        return;
    }
    sys::background_process();
    unsafe {
        let hinst = GetModuleHandleW(null());
        let cls = w(CLASS);
        let mut wc: WNDCLASSEXW = zeroed();
        wc.cbSize = size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = hinst;
        wc.lpszClassName = cls.as_ptr();
        RegisterClassExW(&wc);
        let title = w("NEXO Vigía");
        let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW, cls.as_ptr(), title.as_ptr(), WS_POPUP, 0, 0, 0, 0, null_mut(), null_mut(), hinst, null());
        if hwnd.is_null() {
            return;
        }
        let icon = make_icon(32);
        let tb = w("TaskbarCreated");
        ST.with(|s| {
            *s.borrow_mut() = Some(State {
                hwnd,
                icon,
                sampler: Sampler::default(),
                cfg: Config::load(),
                cfg_mtime: config::config_mtime(),
                last_alert: HashMap::new(),
                last_stabilize: None,
                high: 0,
                crit: 0,
                cpu_hi: 0,
                hist: HashMap::new(),
                paused_until: None,
                taskbar_msg: RegisterWindowMessageW(tb.as_ptr()),
                defender_seen: false,
                defender_missing: 0,
                startup: None,
            })
        });
        tray_add(hwnd, icon);
        SetTimer(hwnd, 1, TICK_MS, None);
        SetTimer(hwnd, 2, 4000, None);
        sys::trim_own_memory();

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let mut n = nid(hwnd);
        n.uFlags = 0;
        Shell_NotifyIconW(NIM_DELETE, &n);
    }
}
