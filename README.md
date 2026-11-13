# NEXO

Limpieza segura, memoria, diagnóstico y protección. 100 % local, sin internet.
Autor: **Chatta Betanzos, Jimy Alexander**.

| Carpeta | Qué es | Plataformas |
|---|---|---|
| `nexo/` | Aplicación de escritorio (Rust + egui) | Windows 10/11, Linux (X11/Wayland), macOS 11+ (Apple Silicon e Intel) |
| `nexo-ios/` | App para iPhone (SwiftUI) | iOS 16+ |
| `nexo-movil/` | App para Android (Kotlin) | Android 11+ |

## Regla de oro (en todas las plataformas)
Nada se borra sin que lo veas. La basura del sistema se elimina; lo tuyo va a la Papelera
(o a «Eliminado recientemente» en iPhone). Los videos y documentos nunca se borran solos.

## Compilar
- **Escritorio:** ver `nexo/README.md` (`cargo build --release`). En Linux y macOS es el mismo
  código: los módulos del sistema operativo están en `nexo/src/unix/`.
- **iPhone:** en una Mac con Xcode: `brew install xcodegen && cd nexo-ios && xcodegen generate && open NEXO.xcodeproj`.
- **Android:** `nexo-movil/` con Android Studio o Gradle 8.14.

## Integración continua
`.github/workflows/nexo.yml` compila y prueba las cuatro plataformas en GitHub Actions y deja
como descargables: `nexo.exe`, `nexo-linux-x86_64.tar.gz`, `NEXO-macOS.dmg`,
`NEXO-iOS-sin-firmar.ipa` y capturas de pantalla de cada sistema.
