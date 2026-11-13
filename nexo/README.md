# NEXO · versión de escritorio (Windows)

Limpieza segura, estabilizador de RAM, diagnóstico y protección para Windows 11, con enlace a celulares Android por ADB. 100 % local.

## Tecnologías

| Capa | Tecnología |
|---|---|
| Frontend (interfaz) | Rust + eframe/egui 0.29 (OpenGL/glow) |
| Backend (núcleo) | Rust 1.99, windows-sys 0.59 / windows 0.62 (Win32), walkdir, trash, serde_json, tauri-winrt-notification |
| Diagnóstico | PowerShell 5.1 (`src/diag.ps1`, incluido en el .exe) |
| Celulares | Android Debug Bridge (platform-tools) |
| Datos | JSON en `%APPDATA%\NEXO` |

## Abrir en Visual Studio Code

1. Instala Rust: <https://rustup.rs> (destino `x86_64-pc-windows-msvc`).
2. Instala **Visual Studio Build Tools** con «Desarrollo para el escritorio con C++».
3. En VS Code: *Archivo › Abrir carpeta…* → esta carpeta (`nexo`).
4. Acepta instalar las extensiones recomendadas (rust-analyzer, C/C++, Even Better TOML).
5. `Ctrl+Mayús+B` compila. `F5` ejecuta con el depurador (sin pedir administrador).

Tareas incluidas (*Terminal › Ejecutar tarea*): compilar, compilar .exe final, ejecutar, ejecutar sin administrador, pruebas, clippy y simular el modo automático.

## Comandos

```powershell
cargo build                  # depuración
cargo test --release         # 58 pruebas automatizadas
cargo clippy --release       # análisis estático (0 advertencias)
cargo run --release          # abre NEXO (pide administrador)
$env:NEXO_SIN_ELEVAR="1"; cargo run --release   # sin pedir administrador
```

## Generar el .exe

```powershell
cargo build --release        # → target\release\nexo.exe (≈ 4,4 MB, sin dependencias)
.\verificar.ps1              # clippy + pruebas + release + simulación del automático
```

El perfil `[profile.release]` de `Cargo.toml` usa LTO, `codegen-units = 1`, `strip` y `panic = "abort"`; `#![windows_subsystem = "windows"]` evita la consola.

## Modos del ejecutable

`nexo.exe` (ventana) · `--pagina <panel|limpieza|automatico|historial|ram|diagnostico|proteccion|celulares>` · `--vigia` · `--auto` · `--informe <archivo>` · `--simular-auto <archivo>` · `--diagnostico <archivo>` · `--instalar` · `--desinstalar <archivo>`.
