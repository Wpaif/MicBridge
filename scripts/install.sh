#!/usr/bin/env bash
# Compila e instala o MicBridge para o usuário (binário, ícone e atalho .desktop).
set -euo pipefail
cd "$(dirname "$0")/../app"
cargo build --release
install -Dm755 target/release/micbridge ~/.local/bin/micbridge
install -Dm644 data/dev.wilian.MicBridge.svg ~/.local/share/icons/hicolor/scalable/apps/dev.wilian.MicBridge.svg
# Exec absoluto: o atalho funciona mesmo sem ~/.local/bin no PATH.
sed -e "s|^Icon=.*|Icon=dev.wilian.MicBridge|" \
    -e "s|^Exec=.*|Exec=$HOME/.local/bin/micbridge|" \
    data/dev.wilian.MicBridge.desktop > ~/.local/share/applications/dev.wilian.MicBridge.desktop
chmod 644 ~/.local/share/applications/dev.wilian.MicBridge.desktop
# PNGs raster além do SVG: alguns shells não renderizam bem SVG no menu.
for sz in 48 64 128 256 512; do
  d=~/.local/share/icons/hicolor/${sz}x${sz}/apps
  mkdir -p "$d"
  rsvg-convert -w $sz -h $sz data/dev.wilian.MicBridge.svg -o "$d/dev.wilian.MicBridge.png"
done
gtk-update-icon-cache -qf ~/.local/share/icons/hicolor 2>/dev/null || true
update-desktop-database ~/.local/share/applications 2>/dev/null || true
echo "Instalado. Procure por MicBridge no menu de aplicativos."
