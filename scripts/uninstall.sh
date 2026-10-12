#!/usr/bin/env bash
# Desinstala o MicBridge do usuário atual.
#
# Por padrão remove só o que o install.sh instala (binário, atalho, ícones e
# autostart) e preserva o pareamento dos celulares e os certificados TLS.
# Use --purge para apagar também esses dados (volta ao estado de fábrica).
set -euo pipefail

PURGE=false
KEEP_CA=false
ASSUME_YES=false

usage() {
  cat <<EOF
Uso: $0 [--purge] [--yes]
  --purge         Remove também celulares pareados e certificados TLS locais.
                  (os celulares terão de parear e instalar a CA de novo)
  --keep-ca       Com --purge, mantém a CA local (ca.pem/ca.key): os dispositivos
                  que já confiam nela continuam confiando após reinstalar.
  --remove-certs  Sinônimo de --purge (compatibilidade).
  --yes, -y       Não pede confirmação.
EOF
}

for arg in "$@"; do
  case "$arg" in
    --purge|--remove-certs) PURGE=true ;;
    --keep-ca) KEEP_CA=true ;;
    --yes|-y) ASSUME_YES=true ;;
    --help|-h) usage; exit 0 ;;
    *) echo "Opção desconhecida: $arg" >&2; usage >&2; exit 1 ;;
  esac
done

APP_ID="dev.wilian.MicBridge"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

BIN="$HOME/.local/bin/micbridge"
DESKTOP="$DATA_HOME/applications/$APP_ID.desktop"
AUTOSTART="$CONFIG_HOME/autostart/$APP_ID.desktop"
ICON_ROOT="$DATA_HOME/icons/hicolor"
PAIRING_DIR="$DATA_HOME/micbridge"
CERT_DIR="$PAIRING_DIR/certs"

if $PURGE && ! $ASSUME_YES; then
  if $KEEP_CA; then
    echo "Isto apaga os celulares pareados e o certificado do servidor (a CA é mantida)."
  else
    echo "Isto apaga os celulares pareados e TODOS os certificados, inclusive a CA."
  fi
  if [[ -t 0 ]]; then
    read -r -p "Continuar? [s/N] " answer
    [[ "$answer" =~ ^[sSyY]$ ]] || { echo "Cancelado."; exit 1; }
  else
    echo "Sem terminal interativo: repita com --yes para confirmar." >&2
    exit 1
  fi
fi

echo "Removendo o MicBridge..."

# Encerra a instância em execução (o microfone virtual e o servidor web
# são filhos dela e terminam junto).
if pgrep -x micbridge >/dev/null 2>&1; then
  pkill -x micbridge || true
  for _ in {1..30}; do
    pgrep -x micbridge >/dev/null 2>&1 || break
    sleep 0.1
  done
  pkill -9 -x micbridge 2>/dev/null || true
  echo "  - instância em execução encerrada"
fi

# Binário, atalho do menu e autostart.
rm -f "$BIN" "$DESKTOP" "$AUTOSTART"

# Ícones: SVG e PNGs raster instalados pelo install.sh.
rm -f "$ICON_ROOT/scalable/apps/$APP_ID.svg"
for sz in 48 64 128 256 512; do
  rm -f "$ICON_ROOT/${sz}x${sz}/apps/$APP_ID.png"
done

if $PURGE; then
  # Pareamento e certificados ficam juntos em ~/.local/share/micbridge.
  if $KEEP_CA && [[ -f "$CERT_DIR/ca.pem" && -f "$CERT_DIR/ca.key" ]]; then
    find "$PAIRING_DIR" -mindepth 1 ! -path "$CERT_DIR" \
      ! -path "$CERT_DIR/ca.pem" ! -path "$CERT_DIR/ca.key" -delete
    echo "  - pareamento e certificado do servidor removidos (CA preservada)"
  else
    rm -rf "$PAIRING_DIR"
    echo "  - pareamento e certificados TLS removidos (CA incluída)"
  fi
fi

# Atualiza os caches, quando as ferramentas existirem.
if command -v gtk-update-icon-cache >/dev/null 2>&1 && [[ -d "$ICON_ROOT" ]]; then
  gtk-update-icon-cache -qf "$ICON_ROOT" 2>/dev/null || true
fi
if command -v update-desktop-database >/dev/null 2>&1 && [[ -d "$DATA_HOME/applications" ]]; then
  update-desktop-database "$DATA_HOME/applications" 2>/dev/null || true
fi

echo "MicBridge desinstalado."
if ! $PURGE; then
  echo "Pareamento e certificados foram preservados (use --purge para apagá-los)."
fi
echo "O código-fonte e as dependências do sistema foram mantidos."
