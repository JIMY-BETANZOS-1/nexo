#!/usr/bin/env bash
# Empaqueta NEXO para Linux en tres formatos a partir de target/release/nexo:
#   nexo-linux-<arq>.tar.gz   ejecutable suelto (cualquier distribución)
#   nexo_<ver>_<deb>.deb      Ubuntu, Debian, Linux Mint, Pop!_OS, Zorin… (doble clic e instalar)
#   NEXO-<arq>.AppImage       cualquier distribución, sin instalar (chmod +x y doble clic)
# Uso: empaquetar/linux.sh   (se ejecuta dentro de nexo/, después de cargo build --release)
set -euo pipefail

ARQ=$(uname -m)                                   # x86_64 | aarch64
case "$ARQ" in x86_64) DEB=amd64 ;; aarch64) DEB=arm64 ;; *) DEB=$ARQ ;; esac
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
BIN=target/release/nexo
SAL=paquetes
rm -rf "$SAL" && mkdir -p "$SAL"
"$BIN" --exportar-icono "$SAL/nexo.png"

ESCRITORIO='[Desktop Entry]
Type=Application
Name=NEXO
GenericName=Optimizador del sistema
Comment=Limpieza segura, memoria, diagnóstico y protección (100 % local)
Exec=nexo
Icon=nexo
Terminal=false
Categories=System;Utility;
Keywords=limpieza;memoria;ram;disco;seguridad;'

# 1) .tar.gz
mkdir -p "$SAL/tar/nexo-linux"
cp "$BIN" "$SAL/tar/nexo-linux/"
cat > "$SAL/tar/nexo-linux/LEEME.txt" <<EOF
NEXO $VER para Linux ($ARQ)

1. chmod +x nexo
2. ./nexo

Al activar la limpieza automática o el vigía, NEXO se copia en ~/.local/share/nexo
y aparece en el menú de aplicaciones. Si prefieres instalarlo como paquete, usa el .deb
(Ubuntu/Debian/Mint) o la AppImage (cualquier distribución).
EOF
tar -czf "$SAL/nexo-linux-$ARQ.tar.gz" -C "$SAL/tar" nexo-linux

# 2) .deb
R="$SAL/deb"
install -Dm755 "$BIN" "$R/usr/bin/nexo"
install -Dm644 "$SAL/nexo.png" "$R/usr/share/icons/hicolor/1024x1024/apps/nexo.png"
mkdir -p "$R/usr/share/applications" "$R/DEBIAN"
printf '%s\n' "$ESCRITORIO" > "$R/usr/share/applications/nexo.desktop"
KB=$(du -sk "$R/usr" | cut -f1)
cat > "$R/DEBIAN/control" <<EOF
Package: nexo
Version: $VER
Section: utils
Priority: optional
Architecture: $DEB
Installed-Size: $KB
Depends: libc6 (>= 2.35), libgl1, libxkbcommon0
Recommends: pkexec | policykit-1, libxkbcommon-x11-0
Maintainer: Jimy Alexander Chatta Betanzos <nexo@localhost>
Homepage: https://github.com/JIMY-BETANZOS-1/nexo
Description: NEXO - optimizador local del sistema
 Limpieza segura de archivos basura, estabilizador de memoria, diagnóstico
 del equipo y protección. Todo se analiza en el propio equipo, sin internet.
 Nunca borra videos ni documentos.
EOF
dpkg-deb --root-owner-group --build "$R" "$SAL/nexo_${VER}_${DEB}.deb"

# 3) AppImage
A="$SAL/NEXO.AppDir"
install -Dm755 "$BIN" "$A/usr/bin/nexo"
cp "$SAL/nexo.png" "$A/nexo.png"
cp "$SAL/nexo.png" "$A/.DirIcon"
printf '%s\n' "$ESCRITORIO" > "$A/nexo.desktop"
cat > "$A/AppRun" <<'EOF'
#!/bin/sh
AQUI="$(dirname "$(readlink -f "$0")")"
exec "$AQUI/usr/bin/nexo" "$@"
EOF
chmod +x "$A/AppRun"
HERR="$SAL/appimagetool"
curl -fsSL -o "$HERR" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARQ.AppImage"
chmod +x "$HERR"
ARCH=$ARQ APPIMAGE_EXTRACT_AND_RUN=1 "$HERR" --no-appstream "$A" "$SAL/NEXO-$ARQ.AppImage"

rm -rf "$SAL/tar" "$R" "$A" "$HERR" "$SAL/nexo.png"
ls -la "$SAL"
