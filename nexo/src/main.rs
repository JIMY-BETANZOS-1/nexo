// Sin ventana de consola: la app gráfica y el modo automático corren en silencio.
#![cfg_attr(windows, windows_subsystem = "windows")]

// NEXO funciona en Windows, Linux y macOS. Los módulos que hablan con el sistema operativo
// tienen una versión por plataforma (Windows: src/*.rs · Linux y macOS: src/unix/*.rs) con
// la misma interfaz; el resto del programa es común.

mod adb;
mod auto;
mod bmp;
mod clean;
mod config;
mod diag;
mod fmt;
mod icon;
#[cfg(windows)]
mod install;
#[cfg(unix)]
#[path = "unix/install.rs"]
mod install;
#[cfg(windows)]
mod notify;
#[cfg(unix)]
#[path = "unix/notify.rs"]
mod notify;
mod procs;
#[cfg(test)]
mod pruebas;
mod rules;
mod scan;
#[cfg(windows)]
mod schedule;
#[cfg(unix)]
#[path = "unix/schedule.rs"]
mod schedule;
#[cfg(windows)]
mod sys;
#[cfg(unix)]
#[path = "unix/sys.rs"]
mod sys;
#[cfg(windows)]
mod thumbs;
#[cfg(unix)]
#[path = "unix/thumbs.rs"]
mod thumbs;
mod ui;
#[cfg(windows)]
mod vigia;
#[cfg(unix)]
#[path = "unix/vigia.rs"]
mod vigia;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--vigia") {
        // Vigía en la bandeja: unos pocos MB, revisa cada 30 s.
        vigia::run();
        return;
    }
    if args.iter().any(|a| a == "--auto") {
        // Lanzado por el Programador de tareas: limpia, avisa y se cierra.
        auto::run();
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--informe") {
        // Diagnóstico de solo lectura: escribe lo que encontraría, sin borrar nada.
        let out = args.get(i + 1).cloned().unwrap_or_else(|| "nexo_informe.txt".into());
        auto::report(std::path::Path::new(&out));
        return;
    }
    // Mantenimiento y comprobaciones (sin interfaz).
    let out_arg = |flag: &str| args.iter().position(|a| a == flag).map(|i| args.get(i + 1).cloned().unwrap_or_else(|| "nexo_salida.txt".into()));
    if args.iter().any(|a| a == "--instalar") {
        let r = install::install();
        if let Some(o) = out_arg("--instalar") {
            let _ = std::fs::write(o, format!("{:?}", r.map(|p| p.display().to_string())));
        }
        return;
    }
    if let Some(o) = out_arg("--desinstalar") {
        let _ = std::fs::write(o, install::uninstall().join("\n"));
        return;
    }
    #[cfg(unix)]
    if let Some(i) = args.iter().position(|a| a == "--exportar-icono") {
        // Empaquetado (macOS/Linux): guarda el icono de NEXO como PNG de 1024 px.
        let out = args.get(i + 1).cloned().unwrap_or_else(|| "nexo.png".into());
        let _ = image::save_buffer(&out, &icon::rgba(1024), 1024, 1024, image::ExtendedColorType::Rgba8);
        return;
    }
    if args.iter().any(|a| a == "--probar-aviso") {
        notify::toast("Prueba de aviso", "Así se verán los avisos de NEXO.", "Es solo una prueba: no pasa nada.");
        std::thread::sleep(std::time::Duration::from_secs(2));
        return;
    }
    if let Some(o) = out_arg("--medir") {
        // Compara el escaneo con prioridad de segundo plano frente a la interactiva.
        let t = |interactive: bool| {
            std::thread::spawn(move || {
                if interactive { sys::interactive_thread() } else { sys::background_thread() }
                let start = std::time::Instant::now();
                auto::report(&std::env::temp_dir().join("nexo_medir.txt"));
                start.elapsed().as_secs_f32()
            })
            .join()
            .unwrap_or(0.0)
        };
        let (bg1, it1, bg2, it2) = (t(false), t(true), t(false), t(true));
        let _ = std::fs::remove_file(std::env::temp_dir().join("nexo_medir.txt"));
        let _ = std::fs::write(o, format!("segundo plano: {:.2} s / {:.2} s\ninteractivo:   {:.2} s / {:.2} s\n", bg1, bg2, it1, it2));
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--miniatura") {
        // Comprobación: guarda la miniatura de un archivo como BMP.
        let (Some(src), Some(dst)) = (args.get(i + 1), args.get(i + 2)) else { return };
        thumbs::init_thread();
        if let Some((w, h, px)) = thumbs::shell_thumbnail(std::path::Path::new(src), 128) {
            let _ = std::fs::write(dst, thumbs::bmp(w, h, &px));
        }
        return;
    }
    if let Some(o) = out_arg("--simular-auto") {
        let _ = std::fs::write(o, auto::simulate());
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--diagnostico") {
        // Diagnóstico de solo lectura volcado a texto (sin interfaz).
        let out = args.get(i + 1).cloned().unwrap_or_else(|| "nexo_diagnostico.txt".into());
        let text = match diag::run() {
            Ok(r) => {
                let mut s = format!("Nota: {}/100 ({})\n", r.overall, diag::grade(r.overall));
                for (a, v) in &r.areas {
                    s += &format!("  {:<15} {}\n", a, v.map_or("—".to_string(), |x| x.to_string()));
                }
                s += &format!("Plan de energía: {} ({})\n\nRecomendaciones:\n", r.power_name, r.power_guid);
                for x in &r.recs {
                    s += &format!("  [{}] {} · {} — {}\n", x.severity, x.area, x.title, x.detail);
                }
                s
            }
            Err(e) => format!("ERROR: {}", e),
        };
        let _ = std::fs::write(out, text);
        return;
    }
    let page = args.iter().position(|a| a == "--pagina").and_then(|i| args.get(i + 1).cloned());
    ui::run(page);
}
