// ════════════════════════════════════════════════════════════════════════
//  NEXO · nexo/src/procs_win.rs
//  Autor: Chatta Betanzos, Jimy Alexander  ·  jimy-betanzos
// ════════════════════════════════════════════════════════════════════════

//! Parte de Windows de `procs`: lista de procesos, ventanas y compactación (API Win32).

// jimy-betanzos · dependencia: biblioteca estándar de Rust (ffi)
use std::ffi::c_void;
// jimy-betanzos · dependencia: biblioteca estándar de Rust (mem)
use std::mem::{size_of, zeroed};

// jimy-betanzos · dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::Foundation::{CloseHandle, BOOL, FILETIME, HWND, LPARAM};
// jimy-betanzos · dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
// jimy-betanzos · dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::System::ProcessStatus::{
    K32EmptyWorkingSet, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
// jimy-betanzos · dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::System::Threading::{
    GetProcessIoCounters, GetProcessTimes, GetSystemTimes, OpenProcess, TerminateProcess, IO_COUNTERS,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
};
// jimy-betanzos · dependencia: funciones de Windows (windows-sys)
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, PostMessageW, GW_OWNER, WM_CLOSE,
};

// jimy-betanzos · dependencia: módulo propio de la interfaz
use super::{JbProcInfo, JbWin};

/// Windows puede compactar (vaciar el conjunto de trabajo de) cada programa por separado.
pub const PER_APP_TRIM: bool = true;

/// jimy-betanzos · Convierte un tiempo de Windows (FILETIME) a número.
fn jb_ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

/// jimy-betanzos · Lista los procesos de Windows con su memoria y CPU.
pub fn jb_list_processes() -> Vec<JbProcInfo> {
    let mut out = Vec::with_capacity(300);
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return out;
        }
        let mut e: PROCESSENTRY32W = zeroed();
        e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            let mut p = JbProcInfo {
                pid: e.th32ProcessID,
                exe: String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase(),
                ..Default::default()
            };
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, p.pid);
            if !h.is_null() {
                p.accessible = true;
                let mut m: PROCESS_MEMORY_COUNTERS_EX = zeroed();
                m.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
                if K32GetProcessMemoryInfo(h, &mut m as *mut _ as *mut PROCESS_MEMORY_COUNTERS, m.cb) != 0 {
                    p.private = m.PrivateUsage as u64;
                    p.working = m.WorkingSetSize as u64;
                }
                let (mut c, mut x, mut k, mut u): (FILETIME, FILETIME, FILETIME, FILETIME) = (zeroed(), zeroed(), zeroed(), zeroed());
                if GetProcessTimes(h, &mut c, &mut x, &mut k, &mut u) != 0 {
                    p.cpu = jb_ft(k) + jb_ft(u);
                }
                let mut io: IO_COUNTERS = zeroed();
                if GetProcessIoCounters(h, &mut io) != 0 {
                    p.io = io.ReadTransferCount + io.WriteTransferCount;
                }
                CloseHandle(h);
            }
            out.push(p);
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    out
}

unsafe extern "system" fn jb_enum_cb(h: HWND, lp: LPARAM) -> BOOL {
    let v = &mut *(lp as *mut Vec<JbWin>);
    if IsWindowVisible(h) == 0 || !GetWindow(h, GW_OWNER).is_null() {
        return 1;
    }
    let len = GetWindowTextLengthW(h);
    if len <= 0 {
        return 1;
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, &mut pid);
    v.push(JbWin { hwnd: h as isize, pid, title: String::from_utf16_lossy(&buf[..n]) });
    1
}

/// jimy-betanzos · Ventanas principales visibles (con título y sin dueño).
pub fn jb_top_windows() -> Vec<JbWin> {
    let mut v: Vec<JbWin> = Vec::new();
    unsafe {
        EnumWindows(Some(jb_enum_cb), &mut v as *mut Vec<JbWin> as LPARAM);
    }
    v
}

/// jimy-betanzos · Proceso de la ventana que el usuario está usando.
pub fn jb_foreground_pid() -> u32 {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
        pid
    }
}

/// jimy-betanzos · (inactivo, total) del sistema en unidades de 100 ns.
pub fn jb_cpu_times() -> Option<(u64, u64)> {
    unsafe {
        let (mut i, mut k, mut u): (FILETIME, FILETIME, FILETIME) = (zeroed(), zeroed(), zeroed());
        if GetSystemTimes(&mut i, &mut k, &mut u) == 0 {
            return None;
        }
        Some((jb_ft(i), jb_ft(k) + jb_ft(u)))
    }
}

/// jimy-betanzos · Compacta (vacía el conjunto de trabajo) de un programa. No lo cierra ni pierde datos.
pub fn jb_trim(g: &super::JbAppGroup) -> u64 {
    let mut freed = 0u64;
    for &pid in &g.pids {
        unsafe {
            let h = OpenProcess(PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                continue;
            }
            let mut m: PROCESS_MEMORY_COUNTERS = zeroed();
            m.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            let before = if K32GetProcessMemoryInfo(h, &mut m, m.cb) != 0 { m.WorkingSetSize as u64 } else { 0 };
            if K32EmptyWorkingSet(h) != 0 && K32GetProcessMemoryInfo(h, &mut m, m.cb) != 0 {
                freed += before.saturating_sub(m.WorkingSetSize as u64);
            }
            CloseHandle(h);
        }
    }
    freed
}

/// jimy-betanzos · Pide al programa que se cierre como si pulsaras la X: puede preguntarte si guardar.
pub fn jb_close_gracefully(g: &super::JbAppGroup) -> usize {
    let mut n = 0;
    for (h, _) in &g.windows {
        unsafe {
            if PostMessageW(*h as *mut c_void, WM_CLOSE, 0, 0) != 0 {
                n += 1;
            }
        }
    }
    n
}

/// jimy-betanzos · Cierre forzado: se pierde lo no guardado. Solo con doble confirmación del usuario.
pub fn jb_force_kill(g: &super::JbAppGroup) -> usize {
    if g.system {
        return 0;
    }
    let mut n = 0;
    for &pid in &g.pids {
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if !h.is_null() {
                if TerminateProcess(h, 1) != 0 {
                    n += 1;
                }
                CloseHandle(h);
            }
        }
    }
    n
}

