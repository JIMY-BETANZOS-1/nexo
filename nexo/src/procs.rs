//! Programas en ejecución: memoria, CPU, actividad de disco y cuán "importantes" son.
//!
//! Nada aquí cierra un programa por su cuenta. Cerrar siempre lo pide el usuario, y
//! primero se intenta por las buenas (WM_CLOSE en Windows, SIGTERM o «Salir» en Linux y
//! macOS): el programa puede preguntar si guardar.
//!
//! La lógica (agrupar, decidir qué se puede compactar, avisos) es común; lo que habla con
//! el sistema operativo está en `procs_win.rs` o en `unix/procs.rs`.

use std::collections::HashMap;
use std::time::Instant;

#[cfg(windows)]
#[path = "procs_win.rs"]
mod plat;
#[cfg(unix)]
#[path = "unix/procs.rs"]
mod plat;

pub use plat::{close_gracefully, force_kill, trim};
#[allow(unused_imports)]
pub use plat::{foreground_pid, list_processes, top_windows};

/// Procesos del sistema: nunca se compactan, cierran ni sugieren.
const SYSTEM_EXES: &[&str] = &[
    "system", "[system process]", "registry", "memory compression", "secure system", "smss.exe",
    "csrss.exe", "wininit.exe", "winlogon.exe", "services.exe", "lsass.exe", "lsaiso.exe",
    "svchost.exe", "dwm.exe", "explorer.exe", "fontdrvhost.exe", "sihost.exe", "ctfmon.exe",
    "msmpeng.exe", "nissrv.exe", "mpdefendercoreservice.exe", "securityhealthservice.exe",
    "securityhealthsystray.exe", "smartscreen.exe", "audiodg.exe", "spoolsv.exe", "taskhostw.exe",
    "runtimebroker.exe", "searchhost.exe", "searchindexer.exe", "startmenuexperiencehost.exe",
    "shellexperiencehost.exe", "textinputhost.exe", "applicationframehost.exe", "wudfhost.exe",
    "dllhost.exe", "conhost.exe", "lockapp.exe", "userinit.exe", "wlanext.exe", "dashost.exe",
    "igfxem.exe", "nexo.exe",
    // Linux y macOS
    "systemd", "kthreadd", "init", "xorg", "xwayland", "gnome-shell", "plasmashell", "kwin_x11",
    "kwin_wayland", "pipewire", "pulseaudio", "wireplumber", "dbus-daemon", "networkmanager",
    "polkitd", "gdm", "sddm", "launchd", "kernel_task", "windowserver", "loginwindow", "finder",
    "dock", "systemuiserver", "mds", "mds_stores", "coreaudiod", "xprotect", "nexo",
];

/// Programas de trabajo: si están abiertos, pueden tener algo sin guardar.
const EDITOR_EXES: &[&str] = &[
    "winword.exe", "excel.exe", "powerpnt.exe", "onenote.exe", "msaccess.exe", "mspub.exe",
    "notepad.exe", "notepad++.exe", "wordpad.exe", "mspaint.exe", "code.exe", "devenv.exe",
    "sublime_text.exe", "idea64.exe", "pycharm64.exe", "studio64.exe", "rider64.exe",
    "photoshop.exe", "illustrator.exe", "afterfx.exe", "adobe premiere pro.exe", "indesign.exe",
    "blender.exe", "obs64.exe", "audacity.exe", "gimp-2.10.exe", "inkscape.exe", "acad.exe",
    "figma.exe", "soffice.bin", "resolve.exe", "capcut.exe", "canva.exe", "kdenlive.exe",
    "spss.exe", "stats.exe", "rstudio.exe", "matlab.exe", "pgadmin4.exe",
    // Linux y macOS
    "code", "gedit", "gnome-text-editor", "kate", "libreoffice", "soffice", "gimp", "inkscape",
    "blender", "obs", "audacity", "kdenlive", "idea", "pycharm", "studio", "rstudio", "textedit",
    "pages", "numbers", "keynote", "xcode", "microsoft word", "microsoft excel",
    "microsoft powerpoint", "final cut pro", "davinci resolve",
];

/// Instaladores y actualizaciones: cerrarlos a medias puede dañar algo.
const INSTALLER_EXES: &[&str] = &[
    "msiexec.exe", "tiworker.exe", "trustedinstaller.exe", "wusa.exe", "setup.exe",
    "dismhost.exe", "mousocoreworker.exe",
    // Linux y macOS
    "apt", "apt-get", "dpkg", "dnf", "rpm", "pacman", "zypper", "packagekitd", "snapd",
    "flatpak", "installer", "softwareupdated",
];

const IO_ACTIVE: f64 = 1.5 * 1024.0 * 1024.0; // bytes/s

/// Nombre comparable entre sistemas: «Chrome.exe», «chrome» y «Google Chrome» se tratan igual.
fn base(exe: &str) -> &str {
    exe.strip_suffix(".exe").unwrap_or(exe)
}

fn listed(list: &[&str], exe: &str) -> bool {
    list.iter().any(|x| base(x) == base(exe))
}

pub fn friendly(exe: &str) -> String {
    let known = match exe {
        "chrome.exe" => "Google Chrome",
        "msedge.exe" => "Microsoft Edge",
        "msedgewebview2.exe" => "Edge WebView (apps)",
        "firefox.exe" => "Firefox",
        "brave.exe" => "Brave",
        "opera.exe" => "Opera",
        "code.exe" => "VS Code",
        "winword.exe" => "Word",
        "excel.exe" => "Excel",
        "powerpnt.exe" => "PowerPoint",
        "onenote.exe" => "OneNote",
        "discord.exe" => "Discord",
        "spotify.exe" => "Spotify",
        "whatsapp.exe" | "whatsapp.root.exe" => "WhatsApp",
        "teams.exe" | "ms-teams.exe" => "Microsoft Teams",
        "explorer.exe" => "Explorador de Windows",
        "svchost.exe" => "Servicios de Windows",
        "msmpeng.exe" => "Microsoft Defender",
        "dwm.exe" => "Composición de ventanas",
        "memory compression" => "Compresión de memoria",
        "searchhost.exe" => "Búsqueda de Windows",
        "onedrive.exe" => "OneDrive",
        "steam.exe" | "steamwebhelper.exe" => "Steam",
        "obs64.exe" => "OBS Studio",
        "zoom.exe" => "Zoom",
        "notepad.exe" => "Bloc de notas",
        "claude.exe" => "Claude",
        "lenovovantage.exe" | "lenovovantageservice.exe" => "Lenovo Vantage",
        "postgres.exe" => "PostgreSQL",
        "node.exe" => "Node.js",
        "java.exe" | "javaw.exe" => "Java",
        "python.exe" | "pythonw.exe" | "python3" | "python" => "Python",
        "chrome" | "google chrome" | "google-chrome" => "Google Chrome",
        "chromium" | "chromium-browser" => "Chromium",
        "firefox" | "firefox-bin" => "Firefox",
        "code" | "code helper" => "VS Code",
        "gnome-shell" => "Escritorio GNOME",
        "plasmashell" => "Escritorio KDE",
        "xorg" | "xwayland" => "Servidor gráfico",
        "windowserver" => "Servidor de ventanas de macOS",
        "kernel_task" => "Núcleo de macOS",
        "safari" => "Safari",
        "finder" => "Finder",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_string();
    }
    let stem = exe.trim_end_matches(".exe");
    let mut c = stem.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => exe.to_string(),
    }
}

#[derive(Clone, Default)]
pub struct ProcInfo {
    pub pid: u32,
    pub exe: String,
    pub private: u64,
    pub working: u64,
    pub cpu: u64,
    pub io: u64,
    pub accessible: bool,
}

pub struct Win {
    pub hwnd: isize,
    pub pid: u32,
    pub title: String,
}

fn looks_unsaved(title: &str) -> bool {
    let t = title.to_lowercase();
    title.starts_with('*')
        || title.contains(" *")
        || title.contains('●')
        || t.contains("sin guardar")
        || t.contains("no guardado")
        || t.contains("unsaved")
        || t.contains("(modificado)")
}

#[derive(Clone, Default)]
pub struct AppGroup {
    pub exe: String,
    pub name: String,
    pub pids: Vec<u32>,
    pub private: u64,
    pub working: u64,
    pub cpu_pct: f32,
    pub io_rate: f64,
    pub windows: Vec<(isize, String)>,
    pub system: bool,
    pub foreground: bool,
    pub unsaved: bool,
    pub editor: bool,
    pub installing: bool,
    pub accessible: bool,
}

impl AppGroup {
    pub fn io_active(&self) -> bool {
        self.io_rate > IO_ACTIVE
    }

    pub fn idle(&self) -> bool {
        self.cpu_pct < 1.5 && !self.io_active()
    }

    /// Se puede compactar sin molestar: segundo plano, inactivo y no del sistema.
    pub fn can_trim(&self) -> bool {
        plat::PER_APP_TRIM && !self.system && !self.foreground && !self.installing && self.accessible && self.idle()
    }

    pub fn reasons(&self) -> Vec<&'static str> {
        let mut r = Vec::new();
        if self.unsaved {
            r.push("Tiene cambios SIN GUARDAR (lo indica el título de su ventana).");
        }
        if self.editor && !self.unsaved {
            r.push("Es un programa de trabajo (documentos, código o edición): puede tener algo sin guardar.");
        }
        if self.installing {
            r.push("Está instalando o actualizando algo: cerrarlo a medias puede dañar la instalación.");
        }
        if self.io_active() {
            r.push("Está leyendo o escribiendo datos ahora mismo (descarga, copia o guardado).");
        }
        if self.foreground {
            r.push("Es la ventana que estás usando en este momento.");
        }
        r
    }
}

/// Toma muestras sucesivas para calcular CPU y actividad de disco por programa.
#[derive(Default)]
pub struct Sampler {
    prev: HashMap<u32, (u64, u64)>,
    prev_t: Option<Instant>,
    sys_prev: Option<(u64, u64)>,
}

impl Sampler {
    /// Devuelve los programas agrupados por ejecutable (ordenados por memoria) y la CPU total.
    pub fn sample(&mut self, with_windows: bool) -> (Vec<AppGroup>, f32) {
        let procs = list_processes();
        let now = Instant::now();
        let dt = self.prev_t.map(|t| now.duration_since(t).as_secs_f64()).unwrap_or(0.0);
        let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
        let fg = foreground_pid();
        let windows = if with_windows { top_windows() } else { Vec::new() };

        let mut groups: HashMap<String, AppGroup> = HashMap::new();
        let mut next = HashMap::with_capacity(procs.len());
        for p in &procs {
            if p.pid == 0 {
                continue;
            }
            let g = groups.entry(p.exe.clone()).or_insert_with(|| AppGroup {
                exe: p.exe.clone(),
                name: friendly(&p.exe),
                system: listed(SYSTEM_EXES, &p.exe),
                editor: listed(EDITOR_EXES, &p.exe),
                installing: listed(INSTALLER_EXES, &p.exe)
                    || (p.exe.contains("setup") || p.exe.contains("install")) && base(&p.exe) != "nexo",
                accessible: true,
                ..Default::default()
            });
            g.pids.push(p.pid);
            g.private += p.private;
            g.working += p.working;
            g.accessible &= p.accessible;
            g.foreground |= p.pid == fg;
            if let (Some(&(c0, io0)), true) = (self.prev.get(&p.pid), dt > 0.2) {
                g.cpu_pct += (p.cpu.saturating_sub(c0) as f64 / (dt * 1e7 * ncpu) * 100.0) as f32;
                g.io_rate += p.io.saturating_sub(io0) as f64 / dt;
            }
            next.insert(p.pid, (p.cpu, p.io));
        }
        for w in windows {
            if let Some(p) = procs.iter().find(|p| p.pid == w.pid) {
                if let Some(g) = groups.get_mut(&p.exe) {
                    g.unsaved |= looks_unsaved(&w.title);
                    g.windows.push((w.hwnd, w.title));
                }
            }
        }
        self.prev = next;
        self.prev_t = Some(now);

        let mut v: Vec<AppGroup> = groups.into_values().collect();
        v.sort_by_key(|g| std::cmp::Reverse(g.private));
        (v, self.system_cpu())
    }

    fn system_cpu(&mut self) -> f32 {
        let Some((idle, total)) = plat::cpu_times() else { return 0.0 };
        let pct = match self.sys_prev {
            Some((pi, pt)) if total > pt => {
                let dt = (total - pt) as f64;
                ((1.0 - idle.saturating_sub(pi) as f64 / dt) * 100.0).clamp(0.0, 100.0) as f32
            }
            _ => 0.0,
        };
        self.sys_prev = Some((idle, total));
        pct
    }
}

/// Igual que `trim_idle`, informando el avance en milésimas (para la barra de progreso).
pub fn trim_idle_progress(groups: &[AppGroup], progress: &std::sync::atomic::AtomicU32) -> (usize, u64) {
    #[cfg(unix)]
    {
        let _ = groups;
        progress.store(300, std::sync::atomic::Ordering::Relaxed);
        return plat::stabilize_system();
    }
    #[allow(unreachable_code)]
    let todo: Vec<&AppGroup> = groups.iter().filter(|g| g.can_trim()).collect();
    let (mut n, mut freed) = (0, 0);
    for (i, g) in todo.iter().enumerate() {
        let f = trim(g);
        if f > 0 {
            n += 1;
            freed += f;
        }
        progress.store(((i + 1) * 1000 / todo.len().max(1)) as u32, std::sync::atomic::Ordering::Relaxed);
    }
    (n, freed)
}

/// Compacta todos los programas inactivos de segundo plano. Devuelve (programas, bytes).
#[cfg_attr(unix, allow(dead_code))]
pub fn trim_idle(groups: &[AppGroup]) -> (usize, u64) {
    let mut n = 0;
    let mut freed = 0;
    for g in groups.iter().filter(|g| g.can_trim()) {
        let f = trim(g);
        if f > 0 {
            n += 1;
            freed += f;
        }
    }
    (n, freed)
}

