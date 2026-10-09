#!/usr/bin/env bash

# Use --remove-certs para apagar também os certificados TLS locais.

set -euo pipefail

REMOVE_CERTS=false

case "${1:-}" in
"")
;;
--remove-certs)
REMOVE_CERTS=true
;;
--help|-h)
echo "Uso: $0 [--remove-certs]"
echo "  --remove-certs  Remove também os certificados TLS locais."
exit 0
;;
*)
echo "Opção desconhecida: $1" >&2
echo "Uso: $0 [--remove-certs]" >&2
exit 1
;;
esac

DATA_DIR="$HOME/.local/share"
BIN="$HOME/.local/bin/micbridge"
DESKTOP="$DATA_DIR/applications/dev.wilian.MicBridge.desktop"
ICON="$DATA_DIR/icons/hicolor/scalable/apps/dev.wilian.MicBridge.svg"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
CERT_DIR="$PROJECT_DIR/web/tmp/certs"

echo "Removendo o MicBridge..."

# Binário e atalho do menu.

rm -f "$BIN"
rm -f "$DESKTOP"

# Ícone SVG.

rm -f "$ICON"

# Ícones PNG instalados pelo install.sh.

for sz in 48 64 128 256 512; do
rm -f "$DATA_DIR/icons/hicolor/${sz}x${sz}/apps/dev.wilian.MicBridge.png"
done

# Atualiza os caches, quando as ferramentas estiverem disponíveis.

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
gtk-update-icon-cache -qf "$DATA_DIR/icons/hicolor" 2>/dev/null || true
fi

if command -v update-desktop-database >/dev/null 2>&1; then
update-desktop-database "$DATA_DIR/applications" 2>/dev/null || true
fi

# Certificados ficam preservados, a menos que o usuário solicite removê-los.

if [[ "$REMOVE_CERTS" == true ]]; then
rm -f 
"$CERT_DIR/ca.pem" 
"$CERT_DIR/ca.key" 
"$CERT_DIR/cert.pem" 
"$CERT_DIR/key.pem" 
"$CERT_DIR/server.csr" 
"$CERT_DIR/ca.srl"

rmdir "$CERT_DIR" 2>/dev/null || true
echo "Certificados TLS locais removidos, se existiam."
fi

echo "MicBridge desinstalado."
echo "O código-fonte e as dependências do sistema foram preservados."
