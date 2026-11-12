#!/usr/bin/env bash
# Compila e instala o MicBridge para o usuário (binário, ícones e atalho .desktop).
# O servidor HTTPS é embutido no app: não há Ruby, Rails nem openssl envolvidos.
#
# Sem opções: instala ou atualiza por cima, preservando pareamento e certificados.
# A interface é QML, aberta pelo Quickshell (`qs`).
#
# --reinstall: instalação limpa. Desinstala tudo (inclusive o pareamento), apaga
#              o build e instala do zero. A CA local é preservada para
#              os dispositivos continuarem confiando; use --new-ca para refazê-la.
set -euo pipefail

REINSTALL=false
ASSUME_YES=false
NEW_CA=false

usage() {
  cat <<EOF
Uso: $0 [--reinstall] [--new-ca] [--yes]
  --reinstall   Instalação limpa: remove tudo, recompila e reinstala do zero.
                Os celulares precisarão parear novamente.
  --new-ca      Com --reinstall, gera também uma CA nova (os dispositivos terão
                de confiar nela de novo).
  --yes, -y     Não pede confirmação no --reinstall.
EOF
}

for arg in "$@"; do
  case "$arg" in
    --reinstall|--clean) REINSTALL=true ;;
    --new-ca) NEW_CA=true ;;
    --yes|-y) ASSUME_YES=true ;;
    # Havia também uma interface GTK; a opção segue aceita e é ignorada.
    --ui=*) echo "Aviso: $arg ignorado; a interface é sempre a do Quickshell." >&2 ;;
    --help|-h) usage; exit 0 ;;
    *) echo "Opção desconhecida: $arg" >&2; usage >&2; exit 1 ;;
  esac
done

APP_ID="dev.wilian.MicBridge"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_DIR="$PROJECT_DIR/app"

missing=()
need() { command -v "$1" >/dev/null 2>&1 || missing+=("$1 ($2)"); }
need cargo "Rust"
need rsvg-convert "librsvg, para os ícones"
need pw-loopback "pipewire-utils, para o microfone virtual"
need pw-cat "pipewire-utils"
need qs "quickshell, para a interface"
if ((${#missing[@]})); then
  echo "Dependências ausentes:" >&2
  printf '  - %s\n' "${missing[@]}" >&2
  exit 1
fi
command -v avahi-publish >/dev/null 2>&1 \
  || echo "Aviso: avahi ausente; o nome micbridge.local não será anunciado (o IP continua funcionando)."

# 1. Remove a instalação anterior. Sempre encerra a instância em execução para
#    o binário poder ser substituído; no --reinstall apaga também os dados.
if $REINSTALL; then
  uninstall_args=(--purge)
  $NEW_CA || uninstall_args+=(--keep-ca)
  $ASSUME_YES && uninstall_args+=(--yes)
  "$PROJECT_DIR/scripts/uninstall.sh" "${uninstall_args[@]}"
  echo "Limpando build e dependências antigas..."
  (cd "$APP_DIR" && cargo clean)
else
  pkill -x micbridge 2>/dev/null || true
fi

# 2. Compila o app.
echo "Compilando..."
(cd "$APP_DIR" && cargo build --release)

# 3. Instala binário, ícones e atalho.
install -Dm755 "$APP_DIR/target/release/micbridge" "$HOME/.local/bin/micbridge"
install -Dm644 "$APP_DIR/data/$APP_ID.svg" "$DATA_HOME/icons/hicolor/scalable/apps/$APP_ID.svg"

# PNGs raster além do SVG: alguns shells não renderizam bem SVG no menu.
for sz in 48 64 128 256 512; do
  d="$DATA_HOME/icons/hicolor/${sz}x${sz}/apps"
  mkdir -p "$d"
  rsvg-convert -w "$sz" -h "$sz" "$APP_DIR/data/$APP_ID.svg" -o "$d/$APP_ID.png"
done

# Interface QML: fica ao lado do pareamento e é sempre substituída por inteiro.
UI_DIR="$DATA_HOME/micbridge/ui"
EXEC="$HOME/.local/bin/micbridge"
rm -rf "$UI_DIR"
mkdir -p "$UI_DIR"
install -m644 "$APP_DIR"/ui/*.qml "$UI_DIR/"
mkdir -p "$UI_DIR/i18n"
install -m644 "$APP_DIR"/ui/i18n/*.json "$UI_DIR/i18n/"

# Exec absoluto: o atalho funciona mesmo sem ~/.local/bin no PATH.
mkdir -p "$DATA_HOME/applications"
sed -e "s|^Icon=.*|Icon=$APP_ID|" \
    -e "s|^Exec=.*|Exec=$EXEC|" \
    "$APP_DIR/data/$APP_ID.desktop" > "$DATA_HOME/applications/$APP_ID.desktop"
chmod 644 "$DATA_HOME/applications/$APP_ID.desktop"

# A janela segue o layout da área de trabalho do Omarchy, como os outros apps:
# lado a lado em dwindle/scrolling, flutuando só em floating. Versões antigas
# forçavam a janela a flutuar; aqui essa regra é removida. Falha não desfaz a instalação.
"$PROJECT_DIR/scripts/hypr-float.sh" remove || true

# Um autostart criado antes (talvez com o antigo --quickshell) aponta para o binário atual.
AUTOSTART="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/$APP_ID.desktop"
[[ -f "$AUTOSTART" ]] && sed -i "s|^Exec=.*|Exec=$EXEC --hidden|" "$AUTOSTART"

gtk-update-icon-cache -qf "$DATA_HOME/icons/hicolor" 2>/dev/null || true
update-desktop-database "$DATA_HOME/applications" 2>/dev/null || true

if $REINSTALL; then
  if $NEW_CA; then
    echo "Reinstalação limpa concluída com CA nova: pareie e confie na CA de novo em cada dispositivo."
  else
    echo "Reinstalação limpa concluída. Pareie os celulares de novo (a CA anterior foi mantida)."
  fi
else
  echo "Instalado. Procure por MicBridge no menu de aplicativos."
fi
