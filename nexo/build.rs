//! Datos del ejecutable de Windows: icono y propiedades del archivo (clic derecho →
//! Propiedades → Detalles): nombre, descripción, versión y derechos de autor.
//! En Linux y macOS no hace nada.
//! Autor: jimy-betanzos (Chatta Betanzos, Jimy Alexander)

#[path = "src/icon.rs"]
#[allow(dead_code)]
mod icon;

/// jimy-betanzos · Incrusta el icono y las propiedades del .exe al compilar en Windows.
fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let ico = out.join("nexo.ico");
    icon::jb_write_ico(&ico).expect("no se pudo crear el icono");
    let mut res = winresource::WindowsResource::new();
    res.set_icon(&ico.to_string_lossy());
    res.set("ProductName", "NEXO");
    res.set("FileDescription", "NEXO - Optimizador local del sistema");
    res.set("CompanyName", "jimy-betanzos");
    res.set("LegalCopyright", "(c) 2026 Chatta Betanzos, Jimy Alexander (jimy-betanzos)");
    res.set("Comments", "Autor: Chatta Betanzos, Jimy Alexander");
    res.set("OriginalFilename", "nexo.exe");
    res.compile().expect("no se pudieron incrustar los recursos de Windows");
}
