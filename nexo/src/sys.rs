// Proyecto: NEXO
// Archivo: sys.rs
// Autor: Chatta Betanzos, Jimy Alexander (jimy-betanzos)

//! Llamadas directas a Windows. Todo es local y barato (sin WMI ni PowerShell).

// jimy-betanzos - dependencia: biblioteca estándar de Rust (collections)
use std::collections::HashSet;
// jimy-betanzos - dependencia: biblioteca estándar de Rust (ffi)
use std::ffi::{c_void, OsStr};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (mem)
use std::mem::{size_of, zeroed};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (os)
use std::os::windows::ffi::OsStrExt;
// jimy-betanzos - dependencia: biblioteca estándar de Rust (path)
use std::path::{Path, PathBuf};
// jimy-betanzos - dependencia: biblioteca estándar de Rust (ptr)
use std::ptr::{null, null_mut};

// jimy-betanzos - dependencia: funciones de Windows (windows-sys)
use windows_sys::core::{GUID, PWSTR};
// jimy-betanzos - dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE, SYSTEMTIME};
// jimy-betanzos - dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
// jimy-betanzos - dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::System::Com::CoTaskMemFree;
// jimy-betanzos - dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
// jimy-betanzos - dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
// jimy-betanzos - dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
// jimy-betanzos - dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
// jimy-betanzos - dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, SetPriorityClass, SetProcessWorkingSetSize, SetThreadPriority,
    PROCESS_MODE_BACKGROUND_BEGIN, THREAD_MODE_BACKGROUND_BEGIN,
};
// jimy-betanzos - dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
// jimy-betanzos - dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::UI::Shell::{
    IsUserAnAdmin, SHEmptyRecycleBinW, SHGetKnownFolderPath, SHQueryRecycleBinW,
    SHQueryUserNotificationState, ShellExecuteW, QUERY_USER_NOTIFICATION_STATE, QUNS_BUSY,
    QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI,
    SHERB_NOSOUND, SHQUERYRBINFO,
};
// jimy-betanzos - dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// jimy-betanzos: Convierte un texto del sistema al formato de Windows (UTF-16).
pub fn jb_wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

/// jimy-betanzos: Convierte un texto al formato de Windows (UTF-16).
fn jb_wstr(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// jimy-betanzos: Indica si NEXO tiene permisos de administrador.
pub fn jb_is_admin() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

/// jimy-betanzos: Nombres de ejecutables en marcha, en minúsculas (p. ej. "chrome.exe").
pub fn jb_running_processes() -> HashSet<String> {
    let mut set = HashSet::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return set;
        }
        let mut e: PROCESSENTRY32W = zeroed();
        e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut e) != 0 {
            loop {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                set.insert(String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase());
                if Process32NextW(snap, &mut e) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    set
}

/// jimy-betanzos: Estado de la energía: batería, porcentaje y cargador.
pub struct JbPower {
    pub on_ac: bool,
    pub percent: Option<u8>,
    pub has_battery: bool,
}

/// jimy-betanzos: Lee el estado de la batería y el cargador.
pub fn jb_power() -> JbPower {
    unsafe {
        let mut s: SYSTEM_POWER_STATUS = zeroed();
        if GetSystemPowerStatus(&mut s) == 0 {
            return JbPower { on_ac: true, percent: None, has_battery: false };
        }
        JbPower {
            on_ac: s.ACLineStatus == 1,
            percent: (s.BatteryLifePercent <= 100).then_some(s.BatteryLifePercent),
            has_battery: s.BatteryFlag != 128 && s.BatteryFlag != 255,
        }
    }
}

/// jimy-betanzos: Verdadero si hay un juego/video en pantalla completa o modo presentación.
pub fn jb_user_busy() -> bool {
    unsafe {
        let mut st: QUERY_USER_NOTIFICATION_STATE = 0;
        if SHQueryUserNotificationState(&mut st) != 0 {
            return false;
        }
        st == QUNS_BUSY || st == QUNS_RUNNING_D3D_FULL_SCREEN || st == QUNS_PRESENTATION_MODE
    }
}

/// jimy-betanzos: (bytes, elementos) en la Papelera de reciclaje de todas las unidades.
pub fn jb_recycle_bin() -> (u64, u64) {
    unsafe {
        let mut i: SHQUERYRBINFO = zeroed();
        i.cbSize = size_of::<SHQUERYRBINFO>() as u32;
        if SHQueryRecycleBinW(null(), &mut i) == 0 {
            let (s, n) = (i.i64Size, i.i64NumItems);
            (s.max(0) as u64, n.max(0) as u64)
        } else {
            (0, 0)
        }
    }
}

/// jimy-betanzos: Vacía la Papelera de reciclaje.
pub fn jb_empty_recycle_bin() -> bool {
    unsafe {
        SHEmptyRecycleBinW(
            null_mut(),
            null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        ) == 0
    }
}

/// jimy-betanzos: Ruta de una carpeta especial de Windows (Descargas, Videos…).
pub fn jb_known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let mut p: PWSTR = null_mut();
        let hr = SHGetKnownFolderPath(id, 0, null_mut(), &mut p);
        let out = if hr == 0 && !p.is_null() {
            let mut len = 0;
            while *p.add(len) != 0 {
                len += 1;
            }
            Some(PathBuf::from(String::from_utf16_lossy(std::slice::from_raw_parts(p, len))))
        } else {
            None
        };
        if !p.is_null() {
            CoTaskMemFree(p as *const c_void);
        }
        out
    }
}

/// jimy-betanzos: (libre, total) en bytes de la unidad que contiene `p`.
pub fn jb_disk_space(p: &Path) -> (u64, u64) {
    unsafe {
        let w = jb_wide(p.as_os_str());
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        if GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) != 0 {
            (avail, total)
        } else {
            (0, 0)
        }
    }
}

/// jimy-betanzos: Estado de la memoria RAM y virtual.
pub struct JbMem {
    pub load: u32,
    pub total: u64,
    pub avail: u64,
    /// Memoria comprometida (RAM + archivo de paginación): si se agota, los programas fallan.
    pub commit_total: u64,
    pub commit_avail: u64,
}

/// jimy-betanzos: Métodos de Mem.
impl JbMem {
    /// jimy-betanzos: Porcentaje de memoria virtual en uso.
    pub fn jb_commit_pct(&self) -> f32 {
        if self.commit_total == 0 {
            0.0
        } else {
            100.0 * (1.0 - self.commit_avail as f32 / self.commit_total as f32)
        }
    }
}

/// jimy-betanzos: Lee el estado de la memoria del equipo.
pub fn jb_memory() -> JbMem {
    unsafe {
        let mut m: MEMORYSTATUSEX = zeroed();
        m.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        GlobalMemoryStatusEx(&mut m);
        JbMem {
            load: m.dwMemoryLoad,
            total: m.ullTotalPhys,
            avail: m.ullAvailPhys,
            commit_total: m.ullTotalPageFile,
            commit_avail: m.ullAvailPageFile,
        }
    }
}

/// jimy-betanzos: Unidad donde está instalado el sistema (C:).
pub fn jb_system_drive() -> PathBuf {
    PathBuf::from(std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()) + "\\")
}

/// jimy-betanzos: Lee un valor de texto del registro del sistema.
pub fn jb_reg_string(sub: &str, val: &str) -> Option<String> {
    unsafe {
        let s = jb_wstr(sub);
        let v = jb_wstr(val);
        let mut buf = [0u16; 256];
        let mut sz = (buf.len() * 2) as u32;
        let r = RegGetValueW(
            HKEY_LOCAL_MACHINE,
            s.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut sz,
        );
        if r == 0 {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
            Some(String::from_utf16_lossy(&buf[..len]))
        } else {
            None
        }
    }
}

/// jimy-betanzos: Entradas de arranque automático ("nombre → comando") de las claves Run y de las
/// carpetas de Inicio. Lectura barata: el vigía la compara cada 30 s.
pub fn jb_startup_entries() -> HashSet<String> {
    // jimy-betanzos - dependencia: funciones de Windows (windows-sys)
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    };
    let mut set = HashSet::new();
    let keys: [(HKEY, &str); 3] = [
        (HKEY_CURRENT_USER, "Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
        (HKEY_LOCAL_MACHINE, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run"),
        (HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run"),
    ];
    for (root, sub) in keys {
        unsafe {
            let mut hk: HKEY = null_mut();
            if RegOpenKeyExW(root, jb_wstr(sub).as_ptr(), 0, KEY_READ, &mut hk) != 0 {
                continue;
            }
            let mut i = 0;
            loop {
                let mut name = [0u16; 512];
                let mut nlen = name.len() as u32;
                let mut data = [0u8; 2048];
                let mut dlen = data.len() as u32;
                let mut kind = 0u32;
                let r = RegEnumValueW(hk, i, name.as_mut_ptr(), &mut nlen, null(), &mut kind, data.as_mut_ptr(), &mut dlen);
                if r != 0 {
                    break;
                }
                let n = String::from_utf16_lossy(&name[..nlen as usize]);
                let words: Vec<u16> = data[..dlen as usize].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                let len = words.iter().position(|&c| c == 0).unwrap_or(words.len());
                set.insert(format!("{} → {}", n, String::from_utf16_lossy(&words[..len])));
                i += 1;
            }
            RegCloseKey(hk);
        }
    }
    let folders = [
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup")),
        std::env::var_os("ProgramData").map(|p| PathBuf::from(p).join("Microsoft\\Windows\\Start Menu\\Programs\\StartUp")),
    ];
    for dir in folders.into_iter().flatten() {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n != "desktop.ini" {
                    set.insert(format!("{} → carpeta Inicio", n));
                }
            }
        }
    }
    set
}

/// jimy-betanzos: Marca y modelo del equipo.
pub fn jb_machine_model() -> String {
    let bios = "HARDWARE\\DESCRIPTION\\System\\BIOS";
    let maker = jb_reg_string(bios, "SystemManufacturer").unwrap_or_default();
    let family = jb_reg_string(bios, "SystemFamily")
        .or_else(|| jb_reg_string(bios, "SystemProductName"))
        .unwrap_or_else(|| "Equipo Windows".into());
    format!("{} {}", maker, family).trim().to_string()
}

/// jimy-betanzos: Vuelve a abrir NEXO pidiendo permiso de administrador.
pub fn jb_relaunch_as_admin() -> bool {
    jb_relaunch_as_admin_with("")
}

/// jimy-betanzos: Vuelve a abrir NEXO con permisos de administrador (aviso de Windows) y los mismos argumentos.
pub fn jb_relaunch_as_admin_with(params: &str) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    unsafe {
        let op = jb_wstr("runas");
        let file = jb_wide(exe.as_os_str());
        let p = jb_wstr(params);
        let h = ShellExecuteW(null_mut(), op.as_ptr(), file.as_ptr(), p.as_ptr(), null(), SW_SHOWNORMAL);
        h as isize > 32
    }
}

/// jimy-betanzos: Para trabajo que el usuario está esperando (escanear, limpiar, diagnosticar):
/// cede CPU a lo que estés usando, pero el disco va a velocidad normal.
pub fn jb_interactive_thread() {
    // jimy-betanzos - dependencia: funciones de Windows (windows-sys)
    use windows_sys::Win32::System::Threading::THREAD_PRIORITY_BELOW_NORMAL;
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

/// jimy-betanzos: Baja la prioridad de CPU y disco del hilo actual: el usuario no nota el trabajo.
/// Solo para tareas desatendidas (modo automático, vigía): en modo interactivo
/// haría muy lento el acceso a disco.
pub fn jb_background_thread() {
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
}

/// jimy-betanzos: Devuelve al sistema la memoria propia que NEXO no está usando (solo su propio proceso).
pub fn jb_trim_own_memory() {
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

/// jimy-betanzos: Baja la prioridad de NEXO para no frenar el equipo.
pub fn jb_background_process() {
    unsafe {
        SetPriorityClass(GetCurrentProcess(), PROCESS_MODE_BACKGROUND_BEGIN);
    }
}

/// jimy-betanzos: Unix (segundos) -> (año, mes, día, hora, minuto) en hora local.
pub fn jb_local_time(unix: u64) -> (u16, u16, u16, u16, u16) {
    unsafe {
        let v = (unix + 11_644_473_600) * 10_000_000;
        let ft = FILETIME { dwLowDateTime: v as u32, dwHighDateTime: (v >> 32) as u32 };
        let mut st: SYSTEMTIME = zeroed();
        let mut lt: SYSTEMTIME = zeroed();
        FileTimeToSystemTime(&ft, &mut st);
        if SystemTimeToTzSpecificLocalTime(null(), &st, &mut lt) == 0 {
            lt = st;
        }
        (lt.wYear, lt.wMonth, lt.wDay, lt.wHour, lt.wMinute)
    }
}

/// jimy-betanzos: Una sola ventana de NEXO: si ya hay una abierta, la trae al frente y devuelve false.
pub fn jb_claim_single_instance() -> bool {
    // jimy-betanzos - dependencia: funciones de Windows (windows-sys)
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    // jimy-betanzos - dependencia: funciones de Windows (windows-sys)
    use windows_sys::Win32::System::Threading::CreateMutexW;
    // jimy-betanzos - dependencia: funciones de Windows (windows-sys)
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE};
    unsafe {
        let name = jb_wstr("Local\\NEXO_INTERFAZ");
        let h = CreateMutexW(null(), 0, name.as_ptr());
        if !h.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            let title = jb_wstr("NEXO");
            let win = FindWindowW(null(), title.as_ptr());
            if !win.is_null() {
                ShowWindow(win, SW_RESTORE);
                SetForegroundWindow(win);
            }
            return false;
        }
        // El mutex se mantiene abierto mientras viva el proceso.
        true
    }
}

/// jimy-betanzos: Comando sin ventana de consola.
pub fn jb_hidden_cmd(program: &str) -> std::process::Command {
    // jimy-betanzos - dependencia: biblioteca estándar de Rust (os)
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new(program);
    c.creation_flags(0x0800_0000);
    c
}

/// jimy-betanzos: Abre un archivo con su programa (fotos, videos, PDF...). Los ejecutables y scripts
/// nunca se lanzan: solo se muestran en el Explorador.
pub fn jb_open_file(p: &Path) {
    const RISKY: &[&str] = &[
        "exe", "msi", "msix", "appx", "bat", "cmd", "com", "scr", "ps1", "vbs", "vbe", "js", "jse",
        "wsf", "hta", "jar", "reg", "lnk", "url", "cpl", "msc", "pif",
    ];
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if RISKY.contains(&ext.as_str()) || ext.is_empty() {
        jb_reveal_in_explorer(p);
    } else {
        let _ = std::process::Command::new("explorer").arg(p).spawn();
    }
}

/// jimy-betanzos: Muestra un archivo seleccionado en el Explorador.
pub fn jb_reveal_in_explorer(p: &Path) {
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", p.display())).spawn();
}
